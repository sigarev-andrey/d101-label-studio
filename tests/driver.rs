use anyhow::Result;
use async_trait::async_trait;
use niimbot_d101::{
    bitmap::Bitmap,
    driver::{Driver, PrintOptions, ProtocolMode},
    protocol::Packet,
    transport::Transport,
};
use std::{
    collections::VecDeque,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

#[derive(Default)]
struct Printer {
    sent: Arc<Mutex<Vec<Packet>>>,
    incoming: VecDeque<Packet>,
    pages: u16,
    copies: u16,
    error: bool,
    status_pages: VecDeque<u16>,
    cancel_on_row: Option<Arc<AtomicBool>>,
    unanswered_first_status: bool,
}
#[async_trait]
impl Transport for Printer {
    async fn send(&mut self, p: &Packet) -> Result<()> {
        self.sent.lock().unwrap().push(p.clone());
        let response = match p.command {
            0xc1 => Some(Packet::new(0xc2, [3])),
            0x40 => Some(Packet::new(0x48, [0x0a, 0])),
            0xa5 => {
                let mut v = vec![0; 13];
                v[11] = 3;
                Some(Packet::new(0xb5, v))
            }
            0x21 => Some(Packet::new(0x31, [1])),
            0x23 => Some(Packet::new(0x33, [1])),
            0x01 => Some(Packet::new(0x02, [1])),
            0x03 => Some(Packet::new(0x04, [1])),
            0x13 => {
                self.copies = u16::from_be_bytes([p.data[4], p.data[5]]);
                Some(Packet::new(0x14, [1]))
            }
            0xe3 => {
                self.pages += self.copies;
                Some(Packet::new(0xe4, [1]))
            }
            0xa3 if self.unanswered_first_status => {
                self.unanswered_first_status = false;
                None
            }
            0xa3 => {
                if self.error {
                    Some(Packet::new(0xdb, [2]))
                } else {
                    let mut d = self
                        .status_pages
                        .pop_front()
                        .unwrap_or(self.pages)
                        .to_be_bytes()
                        .to_vec();
                    d.extend([100, 100]);
                    Some(Packet::new(0xb3, d))
                }
            }
            0xf3 => Some(Packet::new(0xf4, [1])),
            0xda => Some(Packet::new(0xd0, [1])),
            0x84 | 0x85 => {
                if let Some(cancel) = &self.cancel_on_row {
                    cancel.store(true, Ordering::Relaxed);
                }
                None
            }
            _ => None,
        };
        if let Some(p) = response {
            self.incoming.push_back(p);
        }
        Ok(())
    }
    async fn receive(&mut self) -> Result<Packet> {
        Ok(self.incoming.pop_front().expect("No simulated response"))
    }
}
fn page() -> Bitmap {
    Bitmap {
        width: 8,
        height: 2,
        bits: vec![0x80, 0],
    }
}
#[tokio::test(start_paused = true)]
async fn batch_has_one_start_and_end_and_three_pages() {
    let sent = Arc::new(Mutex::new(Vec::new()));
    let mut driver = Driver::new(Printer {
        sent: sent.clone(),
        ..Default::default()
    });
    let mut different = page();
    different.bits[0] = 0x40;
    let started = tokio::time::Instant::now();
    let mut progress = Vec::new();
    driver
        .print(
            &[page(), different, page()],
            &PrintOptions {
                protocol: ProtocolMode::B1,
                ..Default::default()
            },
            Arc::new(AtomicBool::new(false)),
            |n| progress.push(n),
        )
        .await
        .unwrap();
    let sent = sent.lock().unwrap();
    assert_eq!(sent.iter().filter(|p| p.command == 0x01).count(), 1);
    assert_eq!(
        sent.iter().find(|p| p.command == 0x01).unwrap().data,
        [0, 3, 0, 0, 0, 0, 0]
    );
    assert_eq!(sent.iter().filter(|p| p.command == 0x03).count(), 3);
    assert_eq!(sent.iter().filter(|p| p.command == 0xe3).count(), 3);
    assert_eq!(sent.last().unwrap().command, 0xf3);
    assert_eq!(progress, [3]);
    assert_eq!(
        started.elapsed(),
        std::time::Duration::ZERO,
        "No page or initial completion delay"
    );
}
#[tokio::test(start_paused = true)]
async fn auto_negotiates_v4_and_uses_thirteen_byte_size() {
    let sent = Arc::new(Mutex::new(Vec::new()));
    let mut driver = Driver::new(Printer {
        sent: sent.clone(),
        unanswered_first_status: true,
        ..Default::default()
    });
    let started = tokio::time::Instant::now();
    driver
        .print(
            &[page()],
            &PrintOptions::default(),
            Arc::new(AtomicBool::new(false)),
            |_| {},
        )
        .await
        .unwrap();
    let sent = sent.lock().unwrap();
    assert_eq!(
        sent.iter().find(|p| p.command == 0x01).unwrap().data.len(),
        9
    );
    assert_eq!(
        sent.iter().find(|p| p.command == 0x13).unwrap().data.len(),
        13
    );
    assert!(!sent.iter().any(|p| p.command == 0x03));
    assert_eq!(started.elapsed(), std::time::Duration::ZERO);
}
#[tokio::test]
async fn paper_error_aborts_without_replaying_job() {
    let sent = Arc::new(Mutex::new(Vec::new()));
    let mut driver = Driver::new(Printer {
        sent: sent.clone(),
        error: true,
        ..Default::default()
    });
    let error = driver
        .print(
            &[page()],
            &PrintOptions {
                protocol: ProtocolMode::B1,
                ..Default::default()
            },
            Arc::new(AtomicBool::new(false)),
            |_| panic!("Must not complete"),
        )
        .await
        .unwrap_err();
    assert!(error.to_string().contains("нет этикеток"));
    let sent = sent.lock().unwrap();
    assert_eq!(sent.iter().filter(|p| p.command == 0x01).count(), 1);
    assert_eq!(sent.last().unwrap().command, 0xda);
}

#[tokio::test(start_paused = true)]
async fn adjacent_copies_send_one_raster_per_run_in_both_protocols() {
    for mode in [ProtocolMode::B1, ProtocolMode::V4] {
        let sent = Arc::new(Mutex::new(Vec::new()));
        let mut driver = Driver::new(Printer {
            sent: sent.clone(),
            ..Default::default()
        });
        let a = page();
        let mut b = page();
        b.bits[0] = 0x40;
        // Same bytes with different dimensions must not collapse into one run.
        let c = Bitmap {
            width: 16,
            height: 1,
            bits: a.bits.clone(),
        };
        let mut progress = Vec::new();
        driver
            .print(
                &[a.clone(), a.clone(), b.clone(), b, a, c],
                &PrintOptions {
                    protocol: mode,
                    ..Default::default()
                },
                Arc::new(AtomicBool::new(false)),
                |n| progress.push(n),
            )
            .await
            .unwrap();
        let sent = sent.lock().unwrap();
        let sizes: Vec<_> = sent
            .iter()
            .filter(|p| p.command == 0x13)
            .map(|p| p.data.clone())
            .collect();
        assert_eq!(
            sizes
                .iter()
                .map(|d| u16::from_be_bytes([d[4], d[5]]))
                .collect::<Vec<_>>(),
            [2, 2, 1, 1]
        );
        assert_eq!(sizes[3][..4], [0, 1, 0, 16]);
        assert_eq!(
            sent.iter()
                .filter(|p| p.command == 0x85)
                .map(|p| p.data[6])
                .collect::<Vec<_>>(),
            [0x80, 0x40, 0x80, 0x80]
        );
        assert_eq!(sent.iter().filter(|p| p.command == 0xe3).count(), 4);
        assert_eq!(
            sent.iter().find(|p| p.command == 0x01).unwrap().data[..2],
            [0, 6]
        );
        assert_eq!(progress, [6]);
    }
}

#[tokio::test(start_paused = true)]
async fn thousand_copies_use_u16_and_progress_waits_for_physical_completion() {
    let sent = Arc::new(Mutex::new(Vec::new()));
    let mut driver = Driver::new(Printer {
        sent: sent.clone(),
        status_pages: [0, 400, 1000].into(),
        ..Default::default()
    });
    let mut progress = Vec::new();
    let started = tokio::time::Instant::now();
    driver
        .print(
            &vec![page(); 1000],
            &PrintOptions {
                protocol: ProtocolMode::B1,
                ..Default::default()
            },
            Arc::new(AtomicBool::new(false)),
            |n| progress.push(n),
        )
        .await
        .unwrap();
    let sent = sent.lock().unwrap();
    assert_eq!(sent.iter().filter(|p| p.command == 0x13).count(), 1);
    assert_eq!(
        sent.iter().find(|p| p.command == 0x13).unwrap().data[4..6],
        [3, 232]
    );
    assert_eq!(sent.iter().filter(|p| p.command == 0xe3).count(), 1);
    assert_eq!(sent.iter().filter(|p| p.command == 0xa3).count(), 3);
    assert_eq!(progress, [400, 1000]);
    assert_eq!(started.elapsed(), std::time::Duration::from_millis(500));
    assert_eq!(sent.last().unwrap().command, 0xf3);
}

#[tokio::test(start_paused = true)]
async fn cancellation_stops_copy_run_without_replay() {
    let cancel = Arc::new(AtomicBool::new(false));
    let sent = Arc::new(Mutex::new(Vec::new()));
    let mut driver = Driver::new(Printer {
        sent: sent.clone(),
        cancel_on_row: Some(cancel.clone()),
        ..Default::default()
    });
    let error = driver
        .print(
            &vec![
                Bitmap {
                    width: 8,
                    height: 1,
                    bits: vec![0x80]
                };
                10
            ],
            &PrintOptions {
                protocol: ProtocolMode::B1,
                ..Default::default()
            },
            cancel,
            |_| panic!("Incomplete run"),
        )
        .await
        .unwrap_err();
    assert!(error.to_string().contains("отменена"));
    let sent = sent.lock().unwrap();
    assert_eq!(sent.iter().filter(|p| p.command == 0x01).count(), 1);
    assert!(!sent.iter().any(|p| p.command == 0xe3 || p.command == 0xf3));
    assert_eq!(sent.last().unwrap().command, 0xda);
}
