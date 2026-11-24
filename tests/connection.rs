use anyhow::{Result, bail};
use async_trait::async_trait;
use niimbot_d101::{
    bitmap::Bitmap,
    connection::{Connection, Connector},
    driver::PrintOptions,
    protocol::Packet,
    transport::Transport,
};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex, atomic::AtomicBool},
    time::Duration,
};

#[derive(Default)]
struct Record {
    opened: Vec<String>,
    closed: Vec<usize>,
    commands: Vec<(usize, u8)>,
    active: usize,
    max_active: usize,
    fail_next: Option<u8>,
    block: Option<u8>,
    unsupported_rfid: bool,
    malformed_rfid: bool,
    wrong_model: bool,
}
struct Factory(Arc<Mutex<Record>>);
struct Printer {
    record: Arc<Mutex<Record>>,
    id: usize,
    incoming: VecDeque<Packet>,
    pages: u16,
    copies: u16,
    closed: bool,
}
#[async_trait]
impl Connector for Factory {
    type Link = Printer;
    async fn connect(&self, address: &str) -> Result<Printer> {
        let mut record = self.0.lock().unwrap();
        record.opened.push(address.into());
        record.active += 1;
        record.max_active = record.max_active.max(record.active);
        Ok(Printer {
            record: self.0.clone(),
            id: record.opened.len(),
            incoming: VecDeque::new(),
            pages: 0,
            copies: 0,
            closed: false,
        })
    }
}
#[async_trait]
impl Transport for Printer {
    async fn send(&mut self, p: &Packet) -> Result<()> {
        assert!(!self.closed, "write on a closed transport");
        let (block, unsupported, malformed, wrong_model) = {
            let mut r = self.record.lock().unwrap();
            r.commands.push((self.id, p.command));
            if r.fail_next == Some(p.command) {
                r.fail_next = None;
                bail!("link lost");
            }
            (
                r.block == Some(p.command),
                r.unsupported_rfid,
                r.malformed_rfid,
                r.wrong_model,
            )
        };
        if block {
            std::future::pending::<()>().await;
        }
        let reply = match p.command {
            0xc1 => Some(Packet::new(0xc2, [3])),
            0x40 if p.data == [8] => Some(Packet::new(
                0x48,
                if wrong_model {
                    vec![0x10, 0]
                } else {
                    vec![0x0a, 0]
                },
            )),
            0x40 if p.data == [10] => Some(Packet::new(0x4a, [3])),
            0x1a if unsupported => Some(Packet::new(0, [1])),
            0x1a if malformed => Some(Packet::new(0x1b, [1, 2])),
            0x1a => Some(Packet::new(0x1b, [0])),
            0x59 => Some(Packet::new(0x69, [0, 35, 0, 35])),
            0xa5 => {
                let mut data = vec![0; 13];
                data[11] = 3;
                Some(Packet::new(0xb5, data))
            }
            0x21 => Some(Packet::new(0x31, [1])),
            0x23 => Some(Packet::new(0x33, [1])),
            0x01 => {
                self.pages = 0;
                Some(Packet::new(0x02, [1]))
            }
            0x03 => Some(Packet::new(0x04, [1])),
            0x13 => {
                self.copies = u16::from_be_bytes([p.data[4], p.data[5]]);
                Some(Packet::new(0x14, [1]))
            }
            0xe3 => {
                self.pages += self.copies;
                Some(Packet::new(0xe4, [1]))
            }
            0xa3 => {
                let mut data = self.pages.to_be_bytes().to_vec();
                data.extend([100, 100]);
                Some(Packet::new(0xb3, data))
            }
            0xf3 => Some(Packet::new(0xf4, [1])),
            0xda => Some(Packet::new(0xd0, [1])),
            _ => None,
        };
        if let Some(reply) = reply {
            self.incoming.push_back(reply);
        }
        Ok(())
    }
    async fn receive(&mut self) -> Result<Packet> {
        self.incoming
            .pop_front()
            .ok_or_else(|| anyhow::anyhow!("link lost"))
    }
    async fn close(&mut self) -> Result<()> {
        assert!(!self.closed, "duplicate disconnect");
        self.closed = true;
        let mut r = self.record.lock().unwrap();
        r.closed.push(self.id);
        r.active -= 1;
        Ok(())
    }
}
fn connection() -> (Connection<Factory>, Arc<Mutex<Record>>) {
    let record = Arc::new(Mutex::new(Record::default()));
    (Connection::new(Factory(record.clone())), record)
}
fn page() -> Bitmap {
    Bitmap {
        width: 8,
        height: 1,
        bits: vec![128],
    }
}
fn cancel() -> Arc<AtomicBool> {
    Arc::new(AtomicBool::new(false))
}

#[tokio::test]
async fn status_keepalive_and_multiple_jobs_share_one_connection_and_handshake() {
    let (mut c, r) = connection();
    c.read_status("A").await.unwrap();
    c.ping("A").await.unwrap();
    let mut progress = vec![];
    c.print("A", &[page()], &PrintOptions::default(), cancel(), |n| {
        progress.push(n)
    })
    .await
    .unwrap();
    c.read_status("A").await.unwrap();
    c.print(
        "A",
        &[page(), page()],
        &PrintOptions::default(),
        cancel(),
        |n| progress.push(n),
    )
    .await
    .unwrap();
    c.ping("a").await.unwrap();
    assert_eq!(progress, [1, 2]);
    assert!(c.connected_to("A"));
    {
        let r = r.lock().unwrap();
        assert_eq!(r.opened, ["A"]);
        assert!(r.closed.is_empty());
        assert_eq!(r.commands.iter().filter(|(_, cmd)| *cmd == 0xc1).count(), 1);
        assert_eq!(r.commands.iter().filter(|(_, cmd)| *cmd == 0xa5).count(), 1);
        assert_eq!(r.commands.iter().filter(|(_, cmd)| *cmd == 0x01).count(), 2);
    }
    c.disconnect().await;
    assert_eq!(r.lock().unwrap().closed, [1]);
}
#[tokio::test]
async fn failed_print_reconnects_for_status_without_replaying_any_pages() {
    let (mut c, r) = connection();
    c.read_status("A").await.unwrap();
    r.lock().unwrap().fail_next = Some(0xe3);
    assert!(
        c.print("A", &[page()], &PrintOptions::default(), cancel(), |_| {})
            .await
            .is_err()
    );
    assert!(!c.connected_to("A"));
    c.ping("A").await.unwrap();
    c.read_status("A").await.unwrap();
    {
        let r = r.lock().unwrap();
        assert_eq!(r.opened, ["A", "A"]);
        assert_eq!(r.closed, [1]);
        assert_eq!(r.commands.iter().filter(|(_, cmd)| *cmd == 0x01).count(), 1);
        assert_eq!(r.commands.iter().filter(|(_, cmd)| *cmd == 0xda).count(), 1);
        assert_eq!(r.commands.iter().filter(|(_, cmd)| *cmd == 0xc1).count(), 2);
        assert_eq!(r.max_active, 1);
    }
    c.disconnect().await;
}
#[tokio::test]
async fn unsupported_or_malformed_rfid_keeps_the_valid_session() {
    let (mut c, r) = connection();
    r.lock().unwrap().unsupported_rfid = true;
    assert_eq!(c.read_status("A").await.unwrap().battery_percent, Some(75));
    {
        let mut r = r.lock().unwrap();
        r.unsupported_rfid = false;
        r.malformed_rfid = true;
    }
    assert!(c.read_status("A").await.unwrap().roll.is_none());
    c.ping("A").await.unwrap();
    assert_eq!(r.lock().unwrap().opened, ["A"]);
    assert!(r.lock().unwrap().closed.is_empty());
    c.disconnect().await;
}
#[tokio::test]
async fn link_loss_during_telemetry_invalidates_and_reopens_session() {
    let (mut c, r) = connection();
    c.read_status("A").await.unwrap();
    r.lock().unwrap().fail_next = Some(0x40);
    assert!(c.read_status("A").await.is_err());
    assert!(!c.connected_to("A"));
    c.read_status("A").await.unwrap();
    assert_eq!(r.lock().unwrap().opened, ["A", "A"]);
    assert_eq!(r.lock().unwrap().closed, [1]);
    c.disconnect().await;
}
#[tokio::test]
async fn switching_printer_closes_the_old_link_and_shutdown_is_idempotent() {
    let (mut c, r) = connection();
    c.ping("A").await.unwrap();
    c.ping("B").await.unwrap();
    assert_eq!(r.lock().unwrap().closed, [1]);
    assert_eq!(r.lock().unwrap().max_active, 1);
    c.disconnect().await;
    c.disconnect().await;
    assert_eq!(r.lock().unwrap().closed, [1, 2]);
    assert!(!c.connected_to("B"));
}
#[tokio::test]
async fn aborted_operation_can_still_cancel_and_release_the_owned_link() {
    let (mut c, r) = connection();
    r.lock().unwrap().block = Some(0x01);
    assert!(
        tokio::time::timeout(
            Duration::from_millis(40),
            c.print("A", &[page()], &PrintOptions::default(), cancel(), |_| {})
        )
        .await
        .is_err()
    );
    c.disconnect().await;
    let r = r.lock().unwrap();
    assert_eq!(r.closed, [1]);
    assert_eq!(r.commands.iter().filter(|(_, cmd)| *cmd == 0xda).count(), 1);
}
#[tokio::test]
async fn failed_identification_releases_link_before_reconnecting() {
    let (mut c, r) = connection();
    r.lock().unwrap().wrong_model = true;
    assert!(c.ping("A").await.is_err());
    assert_eq!(r.lock().unwrap().closed, [1]);
    r.lock().unwrap().wrong_model = false;
    c.ping("A").await.unwrap();
    assert_eq!(r.lock().unwrap().opened, ["A", "A"]);
    c.disconnect().await;
}
