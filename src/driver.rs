use crate::{
    bitmap::Bitmap,
    printer_status::{PrinterReading, parse_battery, parse_paper, parse_roll},
    protocol::Packet,
    transport::Transport,
};
use crate::{i18n::tr, local_format};
use anyhow::{Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::time::{Instant, sleep, timeout};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProtocolMode {
    #[default]
    Auto,
    B1,
    V4,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PrintOptions {
    pub density: u8,
    pub label_type: u8,
    pub protocol: ProtocolMode,
}
impl Default for PrintOptions {
    fn default() -> Self {
        Self {
            density: 2,
            label_type: 1,
            protocol: ProtocolMode::Auto,
        }
    }
}

pub struct Driver<T: Transport> {
    transport: T,
    active: bool,
    connected: Option<u8>,
    automatic_mode: Option<ProtocolMode>,
}

#[derive(Debug)]
struct CommandRejected(String);
impl std::fmt::Display for CommandRejected {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for CommandRejected {}

impl<T: Transport> Driver<T> {
    pub fn new(transport: T) -> Self {
        Self {
            transport,
            active: false,
            connected: None,
            automatic_mode: None,
        }
    }
    async fn response(&mut self, expected: u8) -> Result<Packet> {
        timeout(Duration::from_secs(5), async {
            loop {
                let p = self.transport.receive().await?;
                if p.command == 0xdb {
                    return Err(CommandRejected(local_format!(
                        "Ошибка D101: {} (код {})",
                        "D101 error: {} (code {})",
                        printer_error(p.data.first().copied().unwrap_or(0)),
                        p.data.first().copied().unwrap_or(0)
                    ))
                    .into());
                }
                if p.command == 0 {
                    return Err(CommandRejected(
                        tr("D101 не поддерживает команду. Проверьте режим протокола").into(),
                    )
                    .into());
                }
                if p.command == expected {
                    return Ok(p);
                }
            }
        })
        .await
        .map_err(|_| {
            anyhow::anyhow!(local_format!(
                "D101 не ответил на команду (ожидался 0x{expected:02X}){}",
                "D101 timed out waiting for 0x{expected:02X}{}",
                if self.active {
                    tr("; результат печати неизвестен")
                } else {
                    ""
                }
            ))
        })?
    }
    async fn request(&mut self, cmd: u8, data: impl Into<Vec<u8>>, expected: u8) -> Result<Packet> {
        self.transport.send(&Packet::new(cmd, data)).await?;
        self.response(expected).await
    }
    async fn ack(&mut self, cmd: u8, data: impl Into<Vec<u8>>, expected: u8) -> Result<()> {
        let p = self.request(cmd, data, expected).await?;
        ensure!(
            p.data.first() == Some(&1),
            local_format!(
                "Принтер отклонил команду 0x{cmd:02X}: {:02X?}",
                "Printer rejected command 0x{cmd:02X}: {:02X?}",
                p.data
            )
        );
        Ok(())
    }
    async fn identify(&mut self) -> Result<u8> {
        if let Some(connected) = self.connected {
            return Ok(connected);
        }
        let connected = self.request(0xc1, [1], 0xc2).await?;
        ensure!(
            matches!(connected.data.first(), Some(1..=3)),
            local_format!(
                "D101 отказал в подключении: {:?}",
                "D101 rejected the connection: {:?}",
                connected.data
            )
        );
        self.verify_model().await?;
        self.connected = Some(connected.data[0]);
        Ok(connected.data[0])
    }
    async fn verify_model(&mut self) -> Result<()> {
        let model = self.request(0x40, [8], 0x48).await?;
        ensure!(
            model.data.as_slice() == [0x0a, 0x00] || model.data.as_slice() == [0x0a],
            local_format!(
                "Ожидался D101 (получен ID {:02X?})",
                "Expected D101 (received ID {:02X?})",
                model.data
            )
        );
        Ok(())
    }
    /// An acknowledged, read-only keepalive on the existing BLE session.
    pub async fn ping(&mut self) -> Result<()> {
        if self.connected.is_none() {
            self.identify().await.map(|_| ())
        } else {
            self.verify_model().await
        }
    }
    pub async fn close(&mut self) -> Result<()> {
        if self.active {
            let _ = timeout(Duration::from_secs(2), self.request(0xda, [1], 0xd0)).await;
        }
        self.connected = None;
        self.automatic_mode = None;
        self.active = false;
        self.transport.close().await
    }
    async fn negotiate(&mut self, mode: ProtocolMode) -> Result<ProtocolMode> {
        let connected = self.identify().await?;
        if mode != ProtocolMode::Auto {
            return Ok(mode);
        }
        if let Some(mode) = self.automatic_mode {
            return Ok(mode);
        }
        if connected == 3 {
            let data = self.request(0xa5, [1], 0xb5).await?;
            ensure!(
                data.data.len() >= 13,
                tr("Неизвестный ответ версии протокола; выберите B1 или V4 вручную")
            );
            let version = u16::from(data.data[11]) * 100 + u16::from(data.data[12]);
            let mode = if version >= 300 {
                ProtocolMode::V4
            } else {
                ProtocolMode::B1
            };
            self.automatic_mode = Some(mode);
            return Ok(mode);
        }
        self.automatic_mode = Some(ProtocolMode::B1);
        Ok(ProtocolMode::B1)
    }
    async fn optional_request(
        &mut self,
        cmd: u8,
        data: impl Into<Vec<u8>>,
        expected: u8,
    ) -> Result<std::result::Result<Packet, String>> {
        match self.request(cmd, data, expected).await {
            Ok(packet) => Ok(Ok(packet)),
            Err(e) if e.downcast_ref::<CommandRejected>().is_some() => Ok(Err(e.to_string())),
            // Transport errors and timeouts invalidate the session, not just this field.
            Err(e) => Err(e),
        }
    }
    /// Read telemetry without starting a print job or changing media/density settings.
    /// The connection owner closes the session on failure or explicit shutdown.
    pub async fn read_status(&mut self) -> Result<PrinterReading> {
        self.identify().await?;
        let mut reading = PrinterReading {
            connection_id: 0,
            model: "NIIMBOT D101".into(),
            battery_percent: None,
            roll: None,
            paper: None,
            catalog: None,
            notices: Vec::new(),
        };
        match self
            .optional_request(0x40, [10], 0x4a)
            .await?
            .and_then(|p| parse_battery(&p.data).map_err(|e| e.to_string()))
        {
            Ok(value) => reading.battery_percent = Some(value),
            Err(e) => reading.notices.push(local_format!(
                "Заряд недоступен: {e:#}",
                "Battery unavailable: {e:#}"
            )),
        }
        match self
            .optional_request(0x1a, [1], 0x1b)
            .await?
            .and_then(|p| parse_roll(&p.data).map_err(|e| e.to_string()))
        {
            Ok(Some(roll)) => {
                reading.roll = Some(roll);
            }
            Ok(None) => reading.notices.push(
                tr("RFID-метка рулона не прочитана. Тип и остаток этикеток неизвестны.").into(),
            ),
            Err(e) => reading.notices.push(local_format!(
                "Данные рулона недоступны: {e:#}",
                "Roll data unavailable: {e:#}"
            )),
        }
        match self.optional_request(0x59, [1], 0x69).await?
            .and_then(|p| parse_paper(&p.data).map_err(|e| e.to_string())) {
            Ok(Some(paper)) => {
                if paper.size().is_none() {
                    reading.notices.push(local_format!("Размер {} не входит в список форматов D101.", "Size {} is outside the D101 catalog.", paper.text()));
                }
                reading.paper = Some(paper);
            }
            Ok(None) => reading.notices.push(tr("Прошивка не сообщает размер этикетки. Выберите его в меню «Настройки → Размер этикетки».").into()),
            Err(e) => reading.notices.push(local_format!("Размер рулона недоступен: {e}", "Roll size unavailable: {e}")),
        }
        Ok(reading)
    }
    pub async fn print<F>(
        &mut self,
        pages: &[Bitmap],
        options: &PrintOptions,
        cancel: Arc<AtomicBool>,
        mut progress: F,
    ) -> Result<()>
    where
        F: FnMut(usize) + Send,
    {
        ensure!(
            !pages.is_empty() && pages.len() <= 1000,
            tr("В задании должно быть 1–1000 этикеток")
        );
        ensure!(
            (1..=3).contains(&options.density),
            tr("Плотность D101: 1–3")
        );
        ensure!(
            matches!(options.label_type, 1 | 5),
            tr("D101 поддерживает этикетки с зазором и прозрачные")
        );
        for page in pages {
            page.validate()?;
        }
        let result = self
            .print_inner(pages, options, &cancel, &mut progress)
            .await;
        if result.is_err() && self.active {
            // No automatic job replay: a timed-out page may already be on paper.
            let _ = timeout(Duration::from_secs(2), self.request(0xda, [1], 0xd0)).await;
            self.active = false;
        }
        // A successful batch keeps BLE and the negotiated protocol alive for the next job.
        // The connection owner discards the session after any failure.
        result
    }
    async fn print_inner<F: FnMut(usize) + Send>(
        &mut self,
        pages: &[Bitmap],
        options: &PrintOptions,
        cancel: &AtomicBool,
        progress: &mut F,
    ) -> Result<()> {
        let mode = self.negotiate(options.protocol).await?;
        ensure!(!cancel.load(Ordering::Relaxed), tr("Печать отменена"));
        if mode == ProtocolMode::V4 {
            self.ack(0x23, [options.label_type], 0x33).await?;
            self.ack(0x21, [options.density], 0x31).await?;
        } else {
            self.ack(0x21, [options.density], 0x31).await?;
            self.ack(0x23, [options.label_type], 0x33).await?;
        }
        let mut start = (pages.len() as u16).to_be_bytes().to_vec();
        start.resize(if mode == ProtocolMode::V4 { 9 } else { 7 }, 0);
        self.active = true;
        self.ack(0x01, start, 0x02).await?;
        // Only adjacent identical rasters share a firmware copy count. This keeps
        // collated jobs (A,B,A,B) and generated sequences in their original order.
        for run in
            pages.chunk_by(|a, b| a.width == b.width && a.height == b.height && a.bits == b.bits)
        {
            let page = &run[0];
            ensure!(!cancel.load(Ordering::Relaxed), tr("Печать отменена"));
            if mode == ProtocolMode::V4 {
                // V4's first status can be unanswered; keep this one-way preamble.
                // BLE transport already paces packets, so no additional page delay.
                self.transport.send(&Packet::new(0xa3, [1])).await?;
            } else {
                self.ack(0x03, [1], 0x04).await?;
            }
            let mut size = (page.height as u16).to_be_bytes().to_vec();
            size.extend_from_slice(&(page.width as u16).to_be_bytes());
            size.extend_from_slice(&(run.len() as u16).to_be_bytes());
            if mode == ProtocolMode::V4 {
                size.resize(13, 0);
            }
            self.ack(0x13, size, 0x14).await?;
            for packet in page.row_packets()? {
                ensure!(!cancel.load(Ordering::Relaxed), tr("Печать отменена"));
                self.transport.send(&packet).await?;
            }
            ensure!(!cancel.load(Ordering::Relaxed), tr("Печать отменена"));
            self.ack(0xe3, [1], 0xe4).await?;
        }
        // PageEnd acknowledgements provide input flow control. Wait for physical completion
        // after delivering all pages: some firmware needs the next page to feed the previous one.
        let mut deadline = Instant::now() + Duration::from_secs(90);
        let mut last_done = 0;
        loop {
            ensure!(!cancel.load(Ordering::Relaxed), tr("Печать отменена"));
            ensure!(
                Instant::now() < deadline,
                tr("D101 не подтвердил завершение печати")
            );
            let status = self.request(0xa3, [1], 0xb3).await?;
            ensure!(status.data.len() >= 4, tr("Некорректный статус печати"));
            if status.data.len() == 10 && status.data[6] != 0 {
                bail!(local_format!(
                    "Ошибка D101: {} (код {})",
                    "D101 error: {} (code {})",
                    printer_error(status.data[6]),
                    status.data[6]
                ));
            }
            let done = u16::from_be_bytes([status.data[0], status.data[1]]) as usize;
            ensure!(
                done <= pages.len(),
                local_format!(
                    "D101 вернул неверный счётчик страниц: {done}",
                    "D101 returned an invalid page count: {done}"
                )
            );
            if done > last_done {
                progress(done);
                last_done = done;
                deadline = Instant::now() + Duration::from_secs(90);
            }
            if done == pages.len() {
                break;
            }
            // Poll immediately once, then wait only while the printer is busy.
            sleep(Duration::from_millis(250)).await;
        }
        self.ack(0xf3, [1], 0xf4).await?;
        self.active = false;
        if mode == ProtocolMode::V4 {
            self.transport.send(&Packet::new(0xdc, [1])).await?;
        }
        Ok(())
    }
}

pub fn printer_error(code: u8) -> &'static str {
    match code {
        1 => tr("открыта крышка"),
        2 => tr("нет этикеток"),
        3 => tr("низкий заряд"),
        5 => tr("отменено на принтере"),
        6 => tr("ошибка данных"),
        7 => tr("перегрев"),
        9 => tr("принтер занят"),
        0x10 => tr("неподходящая бумага"),
        _ => tr("ошибка принтера"),
    }
}
