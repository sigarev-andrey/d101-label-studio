use anyhow::Result;
use async_trait::async_trait;
use niimbot_d101::{driver::Driver, protocol::Packet, transport::Transport};
use std::{
    collections::VecDeque,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

struct Probe {
    script: VecDeque<(Packet, Packet)>,
    response: Option<Packet>,
    closed: Arc<AtomicBool>,
}
#[async_trait]
impl Transport for Probe {
    async fn send(&mut self, request: &Packet) -> Result<()> {
        let (expected, response) = self.script.pop_front().expect("unexpected command");
        assert_eq!(request.command, expected.command);
        assert_eq!(request.data, expected.data);
        self.response = Some(response);
        Ok(())
    }
    async fn receive(&mut self) -> Result<Packet> {
        self.response
            .take()
            .ok_or_else(|| anyhow::anyhow!("disconnected"))
    }
    async fn close(&mut self) -> Result<()> {
        self.closed.store(true, Ordering::Relaxed);
        assert!(self.script.is_empty(), "probe missed requests");
        Ok(())
    }
}
fn probe(
    model: &[u8],
    battery: Option<&[u8]>,
    roll: Option<&[u8]>,
) -> (Driver<Probe>, Arc<AtomicBool>) {
    let mut script = VecDeque::from([
        (Packet::new(0xc1, [1]), Packet::new(0xc2, [2])),
        (Packet::new(0x40, [8]), Packet::new(0x48, model.to_vec())),
    ]);
    if let Some(battery) = battery {
        script.push_back((
            Packet::new(0x40, [10]),
            if battery.is_empty() {
                Packet::new(0, [1])
            } else {
                Packet::new(0x4a, battery.to_vec())
            },
        ));
    }
    if let Some(roll) = roll {
        script.push_back((Packet::new(0x1a, [1]), Packet::new(0x1b, roll.to_vec())));
        script.push_back((
            Packet::new(0x59, [1]),
            Packet::new(
                0x69,
                [
                    0, 16, 1, 64, 1, 0, 20, 1, 144, 0, 112, 0, 140, 0, 0, 0, 0, 0,
                ],
            ),
        ));
    }
    let closed = Arc::new(AtomicBool::new(false));
    (
        Driver::new(Probe {
            script,
            response: None,
            closed: closed.clone(),
        }),
        closed,
    )
}
#[tokio::test]
async fn reads_model_battery_and_roll_without_printing_or_setting_media() {
    let mut roll = vec![1; 8];
    roll.extend([3, b'A', b'B', b'C', 2, b'X', b'Y', 1, 44, 0, 25, 5, 0, 250]);
    let (mut driver, closed) = probe(&[0x0a, 0], Some(&[3]), Some(&roll));
    let status = driver.read_status().await.unwrap();
    assert_eq!(status.model, "NIIMBOT D101");
    assert_eq!(status.battery_percent, Some(75));
    assert_eq!(
        status.paper.unwrap().size(),
        Some(niimbot_d101::label_size::D101LabelSize::W14L40)
    );
    let roll = status.roll.unwrap();
    assert_eq!(roll.type_name(), "Прозрачные");
    assert_eq!(roll.remaining(roll.capacity), Some(225));
    assert!(status.notices.is_empty());
    assert!(!closed.load(Ordering::Relaxed));
    driver.close().await.unwrap();
    assert!(closed.load(Ordering::Relaxed));
}
#[tokio::test]
async fn unsupported_battery_and_missing_tag_preserve_known_model() {
    let (mut driver, closed) = probe(&[0x0a], Some(&[]), Some(&[0]));
    let status = driver.read_status().await.unwrap();
    assert_eq!(status.model, "NIIMBOT D101");
    assert_eq!(status.battery_percent, None);
    assert_eq!(status.roll, None);
    assert_eq!(status.notices.len(), 2);
    assert!(!closed.load(Ordering::Relaxed));
    driver.close().await.unwrap();
    assert!(closed.load(Ordering::Relaxed));
}
#[tokio::test]
async fn malformed_roll_does_not_discard_valid_charge_or_leak_connection() {
    let (mut driver, closed) = probe(&[0x0a, 0], Some(&[100]), Some(&[1, 2]));
    let status = driver.read_status().await.unwrap();
    assert_eq!(status.battery_percent, Some(100));
    assert!(status.roll.is_none());
    assert_eq!(status.notices.len(), 1);
    assert!(!closed.load(Ordering::Relaxed));
    driver.close().await.unwrap();
    assert!(closed.load(Ordering::Relaxed));
}
#[tokio::test]
async fn wrong_model_is_rejected_before_polling_consumables() {
    let (mut driver, closed) = probe(&[0x10, 0], None, None);
    assert!(driver.read_status().await.is_err());
    driver.close().await.unwrap();
    assert!(closed.load(Ordering::Relaxed));
}
