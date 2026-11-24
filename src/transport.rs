use crate::protocol::{Decoder, Packet};
use crate::{i18n::tr, local_format};
use anyhow::{Context, Result, bail, ensure};
use async_trait::async_trait;
use btleplug::{
    api::{Central, Manager as _, Peripheral as _, ScanFilter, WriteType},
    platform::{Manager, Peripheral},
};
use futures::{Stream, StreamExt};
use std::{collections::VecDeque, pin::Pin, time::Duration};
use tokio::time::{Instant, sleep, sleep_until, timeout};
use uuid::Uuid;

pub const SERVICE: Uuid = Uuid::from_u128(0xe7810a7173ae499d8c15faa9aef0c3f2);
pub const CHANNEL: Uuid = Uuid::from_u128(0xbef8d6c99c214c9eb632bd58c1009f9f);

#[derive(Clone, Debug)]
pub struct Device {
    pub name: String,
    pub address: String,
}

pub async fn scan() -> Result<Vec<Device>> {
    timeout(Duration::from_secs(20), async {
        let manager = Manager::new().await?;
        let adapters = manager.adapters().await?;
        ensure!(
            !adapters.is_empty(),
            tr("Bluetooth-адаптер не найден. Включите Bluetooth в Windows")
        );
        let mut found = Vec::new();
        for adapter in adapters {
            adapter.start_scan(ScanFilter::default()).await?;
            sleep(Duration::from_secs(5)).await;
            let peripherals = adapter.peripherals().await;
            let _ = adapter.stop_scan().await;
            for peripheral in peripherals? {
                if let Some(p) = peripheral.properties().await? {
                    let name = p.local_name.unwrap_or_default();
                    if name.to_uppercase().contains("D101") || p.services.contains(&SERVICE) {
                        found.push(Device {
                            name,
                            address: p.address.to_string(),
                        });
                    }
                }
            }
        }
        found.sort_by(|a, b| a.address.cmp(&b.address));
        found.dedup_by(|a, b| a.address == b.address);
        Ok(found)
    })
    .await
    .context(tr("Истекло время поиска Bluetooth"))?
}

#[async_trait]
pub trait Transport: Send {
    async fn send(&mut self, packet: &Packet) -> Result<()>;
    async fn receive(&mut self) -> Result<Packet>;
    async fn close(&mut self) -> Result<()> {
        Ok(())
    }
}

pub struct BleTransport {
    peripheral: Peripheral,
    characteristic: btleplug::api::Characteristic,
    notifications: Pin<Box<dyn Stream<Item = btleplug::api::ValueNotification> + Send>>,
    decoder: Decoder,
    pending: VecDeque<Packet>,
    next_packet_at: Instant,
    cleanup: DisconnectOnDrop,
}

// Connection attempts can be cancelled by a timeout or application shutdown.
// Keep cleanup ownership even before a complete BleTransport has been built.
struct DisconnectOnDrop(Option<Peripheral>);
impl Drop for DisconnectOnDrop {
    fn drop(&mut self) {
        if let Some(peripheral) = self.0.take()
            && let Ok(runtime) = tokio::runtime::Handle::try_current()
        {
            runtime.spawn(async move {
                let _ = timeout(Duration::from_secs(3), peripheral.disconnect()).await;
            });
        }
    }
}
struct StopScanOnDrop(Option<btleplug::platform::Adapter>);
impl Drop for StopScanOnDrop {
    fn drop(&mut self) {
        if let Some(adapter) = self.0.take()
            && let Ok(runtime) = tokio::runtime::Handle::try_current()
        {
            runtime.spawn(async move {
                let _ = adapter.stop_scan().await;
            });
        }
    }
}

impl BleTransport {
    pub async fn connect(address: &str) -> Result<Self> {
        ensure!(
            !address.is_empty(),
            tr("Сначала выберите Bluetooth-принтер в приложении")
        );
        timeout(Duration::from_secs(25), Self::connect_inner(address))
            .await
            .context(tr("Истекло время подключения к D101"))?
    }
    async fn connect_inner(address: &str) -> Result<Self> {
        let manager = Manager::new().await?;
        for adapter in manager.adapters().await? {
            let mut scan_cleanup = StopScanOnDrop(Some(adapter.clone()));
            adapter.start_scan(ScanFilter::default()).await?;
            let mut selected = None;
            for _ in 0..16 {
                for p in adapter.peripherals().await? {
                    if p.address().to_string().eq_ignore_ascii_case(address) {
                        selected = Some(p);
                        break;
                    }
                }
                if selected.is_some() {
                    break;
                }
                sleep(Duration::from_millis(500)).await;
            }
            if adapter.stop_scan().await.is_ok() {
                scan_cleanup.0 = None;
            }
            if let Some(p) = selected {
                let cleanup = DisconnectOnDrop(Some(p.clone()));
                p.connect().await.context(tr(
                    "Не удалось подключиться. Отключите D101 от приложения на телефоне",
                ))?;
                let setup = async {
                    p.discover_services().await?;
                    let characteristic = p
                        .characteristics()
                        .into_iter()
                        .find(|c| c.uuid == CHANNEL && c.service_uuid == SERVICE)
                        .context(tr("У принтера нет ожидаемого BLE-канала Niimbot"))?;
                    let notifications = p.notifications().await?;
                    p.subscribe(&characteristic).await?;
                    Ok::<_, anyhow::Error>((characteristic, notifications))
                }
                .await;
                match setup {
                    Ok((characteristic, notifications)) => {
                        return Ok(Self {
                            peripheral: p,
                            characteristic,
                            notifications,
                            decoder: Decoder::default(),
                            pending: VecDeque::new(),
                            next_packet_at: Instant::now(),
                            cleanup,
                        });
                    }
                    Err(e) => {
                        drop(cleanup);
                        return Err(e);
                    }
                }
            }
        }
        bail!(local_format!(
            "Принтер {address} не найден. Включите его и повторите поиск",
            "Printer {address} was not found. Turn it on and scan again"
        ))
    }
}

#[async_trait]
impl Transport for BleTransport {
    async fn send(&mut self, packet: &Packet) -> Result<()> {
        let mut bytes = packet.encode()?;
        if packet.command == 0xc1 {
            bytes.insert(0, 3);
        }
        // Pace whole Niimbot packets, not individual ATT fragments. Account for time
        // already spent writing / waiting for acknowledgements: do not add a sleep
        // after every write. A 10 ms minimum packet interval protects BLE adapters
        // that lose unacknowledged writes when flooded (niimblue issue #5).
        sleep_until(self.next_packet_at).await;
        self.next_packet_at = Instant::now() + Duration::from_millis(10);
        // 20 bytes works with the default ATT MTU of 23; larger MTUs are not assumed.
        for chunk in bytes.chunks(20) {
            timeout(
                Duration::from_secs(5),
                self.peripheral
                    .write(&self.characteristic, chunk, WriteType::WithoutResponse),
            )
            .await
            .context(tr("Bluetooth: тайм-аут записи"))??;
        }
        Ok(())
    }
    async fn receive(&mut self) -> Result<Packet> {
        loop {
            if let Some(p) = self.pending.pop_front() {
                return Ok(p);
            }
            let notification = self
                .notifications
                .next()
                .await
                .context(tr("Bluetooth-соединение разорвано"))?;
            if notification.uuid == CHANNEL {
                self.pending.extend(self.decoder.feed(&notification.value));
            }
        }
    }
    async fn close(&mut self) -> Result<()> {
        timeout(Duration::from_secs(3), self.peripheral.disconnect())
            .await
            .context(tr("Тайм-аут отключения"))??;
        self.cleanup.0 = None;
        Ok(())
    }
}
