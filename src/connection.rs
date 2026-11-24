//! One reusable printer session. Engine serializes access for polling and printing.
use crate::i18n::tr;
use crate::{
    bitmap::Bitmap,
    driver::{Driver, PrintOptions},
    printer_status::PrinterReading,
    transport::{BleTransport, Transport},
};
use anyhow::{Result, ensure};
use async_trait::async_trait;
use std::sync::{Arc, atomic::AtomicBool};

#[async_trait]
pub trait Connector: Send + Sync {
    type Link: Transport;
    async fn connect(&self, address: &str) -> Result<Self::Link>;
}
pub struct BluetoothConnector;
#[async_trait]
impl Connector for BluetoothConnector {
    type Link = BleTransport;
    async fn connect(&self, address: &str) -> Result<BleTransport> {
        BleTransport::connect(address).await
    }
}
struct Session<T: Transport> {
    address: String,
    driver: Driver<T>,
}
pub struct Connection<C: Connector> {
    connector: C,
    session: Option<Session<C::Link>>,
    generation: u64,
}
impl<C: Connector> Connection<C> {
    pub fn new(connector: C) -> Self {
        Self {
            connector,
            session: None,
            generation: 0,
        }
    }
    pub fn connected_to(&self, address: &str) -> bool {
        self.session
            .as_ref()
            .is_some_and(|s| s.address.eq_ignore_ascii_case(address))
    }
    async fn driver(&mut self, address: &str) -> Result<&mut Driver<C::Link>> {
        ensure!(!address.is_empty(), tr("Принтер не выбран"));
        if !self.connected_to(address) {
            self.disconnect().await;
        }
        if self.session.is_none() {
            let transport = self.connector.connect(address).await?;
            self.generation = self.generation.wrapping_add(1);
            self.session = Some(Session {
                address: address.into(),
                driver: Driver::new(transport),
            });
        }
        Ok(&mut self.session.as_mut().unwrap().driver)
    }
    pub async fn disconnect(&mut self) {
        // Retain ownership across await so shutdown can retry if an operation is cancelled.
        if let Some(session) = &mut self.session {
            let _ = session.driver.close().await;
        }
        self.session = None;
    }
    async fn finish<R>(&mut self, result: Result<R>) -> Result<R> {
        if result.is_err() {
            self.disconnect().await;
        }
        result
    }
    pub async fn ping(&mut self, address: &str) -> Result<()> {
        let result = async { self.driver(address).await?.ping().await }.await;
        self.finish(result).await
    }
    pub async fn read_status(&mut self, address: &str) -> Result<PrinterReading> {
        let result = async {
            let mut reading = self.driver(address).await?.read_status().await?;
            reading.connection_id = self.generation;
            Ok(reading)
        }
        .await;
        self.finish(result).await
    }
    pub async fn print<F: FnMut(usize) + Send>(
        &mut self,
        address: &str,
        pages: &[Bitmap],
        options: &PrintOptions,
        cancel: Arc<AtomicBool>,
        progress: F,
    ) -> Result<()> {
        let result = async {
            self.driver(address)
                .await?
                .print(pages, options, cancel, progress)
                .await
        }
        .await;
        // Reconnect on the next operation; never replay an uncertain print job here.
        self.finish(result).await
    }
}
