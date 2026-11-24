//! NIIMBOT framing. Notifications are a byte stream, not packet boundaries.
use crate::i18n::tr;
use anyhow::{Result, ensure};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Packet {
    pub command: u8,
    pub data: Vec<u8>,
}

impl Packet {
    pub fn new(command: u8, data: impl Into<Vec<u8>>) -> Self {
        Self {
            command,
            data: data.into(),
        }
    }

    pub fn encode(&self) -> Result<Vec<u8>> {
        // PrintBitmapRowGray always has a two-byte length, even for short rows.
        // Printer notifications still use the ordinary one-byte length framing.
        let mut out = vec![0x55, 0x55, self.command];
        if self.command == 0x8d {
            ensure!(
                self.data.len() <= u16::MAX as usize,
                tr("Слишком большой пакет Niimbot")
            );
            out.extend_from_slice(&(self.data.len() as u16).to_be_bytes());
        } else {
            ensure!(self.data.len() <= 255, tr("Слишком большой пакет Niimbot"));
            out.push(self.data.len() as u8);
        }
        out.extend_from_slice(&self.data);
        let checksum = out[2..].iter().fold(0, |a, b| a ^ b);
        out.extend_from_slice(&[checksum, 0xaa, 0xaa]);
        Ok(out)
    }
}

#[derive(Default)]
pub struct Decoder {
    buffer: Vec<u8>,
}

impl Decoder {
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<Packet> {
        let mut packets = Vec::new();
        self.buffer.extend_from_slice(bytes);
        loop {
            let Some(start) = self.buffer.windows(2).position(|x| x == [0x55, 0x55]) else {
                let tail = self.buffer.last().copied();
                self.buffer.clear();
                if tail == Some(0x55) {
                    self.buffer.push(0x55);
                }
                break;
            };
            self.buffer.drain(..start);
            if self.buffer.len() < 4 {
                break;
            }
            let n = self.buffer[3] as usize;
            let end = n + 7;
            if self.buffer.len() < end {
                break;
            }
            let checksum = self.buffer[2..4 + n].iter().fold(0, |a, b| a ^ b);
            if self.buffer[end - 2..end] != [0xaa, 0xaa] || checksum != self.buffer[end - 3] {
                self.buffer.remove(0);
                continue;
            }
            packets.push(Packet::new(self.buffer[2], self.buffer[4..4 + n].to_vec()));
            self.buffer.drain(..end);
        }
        packets
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn known_density_frame() {
        assert_eq!(
            Packet::new(0x21, [2]).encode().unwrap(),
            [0x55, 0x55, 0x21, 1, 2, 0x22, 0xaa, 0xaa]
        );
    }
    #[test]
    fn grayscale_wire_vectors_use_two_byte_length() {
        assert_eq!(
            Packet::new(0x8d, [0x84, 0, 4, 2, 0]).encode().unwrap(),
            [0x55, 0x55, 0x8d, 0, 5, 0x84, 0, 4, 2, 0, 0x0a, 0xaa, 0xaa]
        );
        assert_eq!(
            Packet::new(0x8d, [0x86, 1, 2, 1, 0, 0x12, 0x34, 0x56, 0x78])
                .encode()
                .unwrap(),
            [
                0x55, 0x55, 0x8d, 0, 9, 0x86, 1, 2, 1, 0, 0x12, 0x34, 0x56, 0x78, 8, 0xaa, 0xaa
            ]
        );
        let large = Packet::new(0x8d, vec![0; 256]).encode().unwrap();
        assert_eq!(&large[..5], &[0x55, 0x55, 0x8d, 1, 0]);
        assert_eq!(large[261], 0x8c);
        assert!(Packet::new(0x85, vec![0; 256]).encode().is_err());
        assert!(Packet::new(0x8d, vec![0; 65536]).encode().is_err());
    }
    #[test]
    fn every_split_and_coalesced_frames() {
        let a = Packet::new(0x31, [1]);
        let b = Packet::new(0xb3, [0, 2, 100, 100]);
        let bytes = [a.encode().unwrap(), b.encode().unwrap()].concat();
        for split in 0..=bytes.len() {
            let mut d = Decoder::default();
            let mut got = d.feed(&bytes[..split]);
            got.extend(d.feed(&bytes[split..]));
            assert_eq!(got, [a.clone(), b.clone()]);
        }
    }
    #[test]
    fn corrupted_frame_resynchronizes() {
        let p = Packet::new(0x31, [1]);
        let mut bad = p.encode().unwrap();
        bad[4] ^= 1;
        bad.extend(p.encode().unwrap());
        assert_eq!(Decoder::default().feed(&bad), [p]);
    }
}
