//! Read-only printer telemetry. Wire layouts: NiimBlueLib packet_parser.ts;
//! the upstream MIT notice and pinned revision are in THIRD_PARTY_NOTICES.md.
use crate::catalog::RollMetadata;
use crate::{bitmap::LabelSpec, label_size::D101LabelSize};
use crate::{i18n::tr, local_format};
use anyhow::{Context, Result, ensure};
use std::time::Instant;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RollInfo {
    pub uid: [u8; 8],
    pub barcode: String,
    pub print_limit: u16,
    pub capacity: Option<u16>,
    pub used: u16,
    pub label_type: u8,
}
impl RollInfo {
    pub fn remaining(&self, capacity: Option<u16>) -> Option<u16> {
        // Only known, discrete media types have a count of individual labels.
        if !matches!(self.label_type, 1 | 2 | 4 | 5 | 6 | 10) {
            None
        } else {
            capacity?.checked_sub(self.used)
        }
    }
    pub fn type_name(&self) -> String {
        match self.label_type {
            1 => tr("С зазором").into(),
            2 => tr("Чёрные").into(),
            3 => tr("Непрерывная лента").into(),
            4 => tr("Перфорированные").into(),
            5 => tr("Прозрачные").into(),
            6 => tr("ПВХ-бирки").into(),
            10 => tr("С чёрной меткой").into(),
            11 => tr("Термоусадочная трубка").into(),
            n => local_format!("Неизвестный тип ({n})", "Unknown type ({n})"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrinterReading {
    pub connection_id: u64,
    pub model: String,
    pub battery_percent: Option<u8>,
    pub roll: Option<RollInfo>,
    pub paper: Option<PaperInfo>,
    pub catalog: Option<RollMetadata>,
    pub notices: Vec<String>,
}
impl PrinterReading {
    pub fn paper_info(&self) -> Option<PaperInfo> {
        self.paper.or_else(|| {
            let size = self.catalog.as_ref()?.size?;
            let (width_tenths, length_tenths) = size.tenths();
            Some(PaperInfo {
                width_tenths,
                length_tenths,
                label_type: self.roll.as_ref()?.label_type,
            })
        })
    }
    pub fn capacity(&self) -> Option<u16> {
        self.roll
            .as_ref()?
            .capacity
            .or_else(|| self.catalog.as_ref()?.capacity)
    }
    pub fn remaining(&self) -> Option<u16> {
        self.roll.as_ref()?.remaining(self.capacity())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PaperInfo {
    pub width_tenths: u16,
    pub length_tenths: u16,
    pub label_type: u8,
}
impl PaperInfo {
    pub fn size(self) -> Option<D101LabelSize> {
        D101LabelSize::from_tenths(self.width_tenths, self.length_tenths)
    }
    pub fn text(self) -> String {
        local_format!(
            "{} × {} мм",
            "{} × {} mm",
            self.width_tenths as f32 / 10.0,
            self.length_tenths as f32 / 10.0
        )
    }
}

pub fn parse_paper(data: &[u8]) -> Result<Option<PaperInfo>> {
    // Legacy four-byte replies have no documented millimetre dimensions.
    if data.len() == 4 {
        return Ok(None);
    }
    ensure!(data.len() == 18, tr("Неизвестный формат размера рулона"));
    let paper = PaperInfo {
        width_tenths: u16::from_be_bytes([data[11], data[12]]),
        length_tenths: u16::from_be_bytes([data[7], data[8]]),
        label_type: data[4],
    };
    // D101 does not use the B21 Pro convention that includes the gap in total height.
    ensure!(
        paper.width_tenths > 0 && paper.length_tenths > 0,
        tr("Принтер вернул нулевой размер рулона")
    );
    Ok(Some(paper))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SizeProposal {
    connection_id: u64,
    roll: Option<(String, [u8; 8])>,
    pub size: D101LabelSize,
    pub outline: crate::label_outline::LabelOutline,
}
impl SizeProposal {
    fn from_status(status: &PrinterStatus) -> Option<Self> {
        if status.phase != StatusPhase::Available || status.printing {
            return None;
        }
        let r = status.reading.as_ref()?;
        let paper = r.paper_info()?;
        if !matches!(paper.label_type, 1 | 5) {
            return None;
        }
        Some(Self {
            connection_id: r.connection_id,
            roll: r.roll.as_ref().map(|r| (r.barcode.clone(), r.uid)),
            size: paper.size()?,
            outline: r
                .catalog
                .as_ref()
                .filter(|m| m.size == paper.size())
                .and_then(|m| m.outline.clone())
                .unwrap_or_default(),
        })
    }
    pub fn still_valid(&self, status: &PrinterStatus) -> bool {
        self.refreshed(status).is_some()
    }
    fn same_roll(&self, other: &Self) -> bool {
        self.connection_id == other.connection_id
            && self.roll == other.roll
            && self.size == other.size
    }
    pub fn refreshed(&self, status: &PrinterStatus) -> Option<Self> {
        let current = Self::from_status(status)?;
        self.same_roll(&current).then_some(current)
    }
}
#[derive(Default)]
pub struct SizeProposalTracker {
    seen: Option<SizeProposal>,
    offered: bool,
}
impl SizeProposalTracker {
    /// Call only when the editor can show a dialog. A decline lasts for this roll/session.
    pub fn offer(&mut self, status: &PrinterStatus, spec: &LabelSpec) -> Option<SizeProposal> {
        let proposal = SizeProposal::from_status(status)?;
        if self
            .seen
            .as_ref()
            .is_some_and(|old| old.same_roll(&proposal))
        {
            if self.offered {
                return None;
            }
        } else {
            self.offered = false;
        }
        if self.seen.as_ref() == Some(&proposal) {
            return None;
        }
        self.seen = Some(proposal.clone());
        let needed = D101LabelSize::from_spec(spec) != Some(proposal.size)
            || spec.outline != proposal.outline;
        self.offered = needed;
        needed.then_some(proposal)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatusPhase {
    NotConfigured,
    Simulation,
    Waiting,
    Connecting,
    Reconnecting,
    Updating,
    Available,
    Unavailable,
}

#[derive(Clone, Debug)]
pub struct PrinterStatus {
    pub phase: StatusPhase,
    pub reading: Option<PrinterReading>,
    pub updated: Option<Instant>,
    pub error: Option<String>,
    pub printing: bool,
}
impl PrinterStatus {
    pub fn new(simulated: bool, configured: bool) -> Self {
        Self {
            phase: if simulated {
                StatusPhase::Simulation
            } else if configured {
                StatusPhase::Waiting
            } else {
                StatusPhase::NotConfigured
            },
            reading: None,
            updated: None,
            error: None,
            printing: false,
        }
    }
    pub fn model_text(&self) -> String {
        if self.phase == StatusPhase::Simulation {
            return tr("D101 · симуляция").into();
        }
        self.reading
            .as_ref()
            .map(|r| r.model.clone())
            .unwrap_or_else(|| tr("Не определена").into())
    }
    pub fn battery_text(&self) -> String {
        self.reading
            .as_ref()
            .and_then(|r| r.battery_percent)
            .map(|p| format!("{p} %"))
            .unwrap_or_else(|| tr("Нет данных").into())
    }
    pub fn media_text(&self) -> String {
        let kind = self
            .reading
            .as_ref()
            .and_then(|r| r.roll.as_ref())
            .map(RollInfo::type_name)
            .unwrap_or_else(|| tr("Не определены").into());
        match self.reading.as_ref().and_then(PrinterReading::paper_info) {
            Some(paper) => format!("{kind} · {}", paper.text()),
            None => kind,
        }
    }
    pub fn remaining_text(&self) -> String {
        if self.printing {
            return tr("После печати").into();
        }
        self.reading
            .as_ref()
            .and_then(|r| {
                Some(local_format!(
                    "{} из {} шт.",
                    "{} of {} labels",
                    r.remaining()?,
                    r.capacity()?
                ))
            })
            .unwrap_or_else(|| tr("Нет данных").into())
    }
    pub fn state_text(&self) -> String {
        if self.printing {
            return tr("Печать…").into();
        }
        match self.phase {
            StatusPhase::NotConfigured => tr("Принтер не выбран").into(),
            StatusPhase::Simulation => tr("Симуляция").into(),
            StatusPhase::Waiting => tr("Ожидание данных").into(),
            StatusPhase::Connecting => tr("Подключение…").into(),
            StatusPhase::Reconnecting => tr("Переподключение…").into(),
            StatusPhase::Updating => tr("Обновление…").into(),
            StatusPhase::Unavailable => tr("Нет связи · повтор через 5 с").into(),
            StatusPhase::Available => tr("Подключён").into(),
        }
    }
    pub fn details(&self) -> String {
        let mut lines = vec![
            self.state_text(),
            tr("Bluetooth-соединение сохраняется до закрытия приложения. Проверка связи — каждые 5 секунд; при обрыве выполняется переподключение.").into(),
            tr("Данные считываются с принтера каждые 30 секунд и после печати.").into(),
        ];
        if let Some(updated) = self.updated {
            lines.push(local_format!(
                "Данные обновлены {} с назад.",
                "Updated {} seconds ago.",
                updated.elapsed().as_secs()
            ));
        }
        if self.phase == StatusPhase::Simulation {
            lines.push(tr("В симуляции принтер не опрашивается.").into());
        }
        if let Some(error) = &self.error {
            lines.push(error.clone());
        }
        if let Some(reading) = &self.reading {
            lines.extend(reading.notices.iter().cloned());
            if let Some(roll) = &reading.roll {
                if !roll.barcode.is_empty() {
                    lines.push(local_format!(
                        "Код рулона: {}",
                        "Roll code: {}",
                        roll.barcode
                    ));
                }
                lines.push(local_format!(
                    "Израсходовано по RFID: {}. Лимит печати RFID: {} (это не вместимость рулона).",
                    "RFID usage: {}. RFID print limit: {}. The limit can exceed roll capacity.",
                    roll.used,
                    roll.print_limit
                ));
                lines.push(if roll.capacity.is_some() {
                    tr("Вместимость получена из RFID принтера.").into()
                } else if let Some(metadata) = &reading.catalog {
                    local_format!("Размер и номинальная вместимость: каталог NIIMBOT, {}. Расход — только из принтера.", "Size and nominal capacity from the NIIMBOT catalog: {}. Usage comes from the printer.", metadata.name)
                } else {
                    tr("Принтер не передал вместимость. Ожидание каталога по коду рулона.").into()
                });
                lines.push(tr("Задания приложения не изменяют остаток.").into());
            }
        }
        lines.join("\n")
    }
}

pub fn parse_battery(data: &[u8]) -> Result<u8> {
    ensure!(
        data.len() == 1 && data[0] <= 100,
        tr("Некорректный ответ заряда батареи")
    );
    Ok(if data[0] <= 4 { data[0] * 25 } else { data[0] })
}

pub fn parse_roll(data: &[u8]) -> Result<Option<RollInfo>> {
    // A single byte reports no readable RFID tag; it does not prove there is no paper.
    if data.len() == 1 {
        return Ok(None);
    }
    ensure!(data.len() >= 15, tr("Неполный ответ RFID"));
    let mut rest = &data[8..];
    fn string(rest: &mut &[u8]) -> Result<String> {
        let (&length, tail) = rest.split_first().context(tr("Нет длины строки RFID"))?;
        let value = tail
            .get(..length as usize)
            .context(tr("Неполная строка RFID"))?;
        *rest = &tail[length as usize..];
        Ok(String::from_utf8_lossy(value)
            .chars()
            .filter(|c| !c.is_control())
            .collect())
    }
    let barcode = string(&mut rest)?;
    let _serial = string(&mut rest)?;
    ensure!(
        matches!(rest.len(), 5 | 7 | 15),
        tr("Неизвестный формат счётчиков RFID")
    );
    Ok(Some(RollInfo {
        uid: data[..8].try_into().unwrap(),
        barcode,
        print_limit: u16::from_be_bytes([rest[0], rest[1]]),
        capacity: rest
            .get(5..7)
            .map(|v| u16::from_be_bytes([v[0], v[1]]))
            .filter(|n| *n > 0 && *n < 0x8000),
        used: u16::from_be_bytes([rest[2], rest[3]]),
        label_type: rest[4],
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    // Synthetic RFID payload with the same field widths as a legacy reply.
    const RFID: &[u8] =
        b"\x01\x02\x03\x04\x05\x06\x07\x08\x08TEST0001\x10SYNTHETIC0000001\x01\x14\x00\x99\x01";
    #[test]
    fn battery_handles_steps_percentages_and_invalid_values() {
        for (raw, expected) in [
            (0, 0),
            (1, 25),
            (2, 50),
            (3, 75),
            (4, 100),
            (60, 60),
            (100, 100),
        ] {
            assert_eq!(parse_battery(&[raw]).unwrap(), expected);
        }
        for invalid in [&[][..], &[255], &[2, 3]] {
            assert!(parse_battery(invalid).is_err());
        }
    }
    #[test]
    fn synthetic_rfid_counts_and_optional_fields() {
        for extension in [vec![], vec![0, 230], vec![0, 230, 1, 2, 3, 4, 5, 6, 7, 8]] {
            let mut bytes = RFID.to_vec();
            let has_capacity = !extension.is_empty();
            bytes.extend(extension);
            let roll = parse_roll(&bytes).unwrap().unwrap();
            assert_eq!(
                (roll.print_limit, roll.used, roll.remaining(roll.capacity)),
                (276, 153, if has_capacity { Some(77) } else { None })
            );
            assert_eq!(roll.barcode, "TEST0001");
            assert_eq!(roll.type_name(), "С зазором");
        }
    }
    #[test]
    fn missing_tag_truncation_and_inconsistent_counters_are_not_zero_stock() {
        assert_eq!(parse_roll(&[0]).unwrap(), None);
        for n in 0..RFID.len() {
            if n != 1 {
                assert!(parse_roll(&RFID[..n]).is_err(), "prefix {n}");
            }
        }
        let mut bad = RFID.to_vec();
        bad[8] = 255;
        assert!(parse_roll(&bad).is_err());
        let mut roll = parse_roll(RFID).unwrap().unwrap();
        roll.used = 231;
        assert_eq!(roll.remaining(Some(230)), None);
        roll.used = 230;
        assert_eq!(roll.remaining(Some(230)), Some(0));
        roll.label_type = 3;
        assert_eq!(roll.remaining(Some(230)), None);
        roll.label_type = 0;
        assert_eq!(roll.remaining(Some(230)), None);
    }
    #[test]
    fn unknown_and_simulated_status_never_invent_battery_or_stock() {
        for status in [
            PrinterStatus::new(false, false),
            PrinterStatus::new(true, true),
        ] {
            assert_eq!(status.battery_text(), "Нет данных");
            assert_eq!(status.remaining_text(), "Нет данных");
            assert_eq!(status.media_text(), "Не определены");
        }
        let mut status = PrinterStatus::new(false, true);
        status.reading = Some(PrinterReading {
            connection_id: 1,
            model: "NIIMBOT D101".into(),
            battery_percent: Some(0),
            roll: parse_roll(RFID).unwrap(),
            paper: None,
            catalog: None,
            notices: vec![],
        });
        assert_eq!(status.battery_text(), "0 %");
        assert_eq!(status.remaining_text(), "Нет данных");
        status.printing = true;
        assert_eq!(status.remaining_text(), "После печати");
    }
    fn sized_status() -> PrinterStatus {
        PrinterStatus {
            phase: StatusPhase::Available,
            reading: Some(PrinterReading {
                connection_id: 1,
                model: "NIIMBOT D101".into(),
                battery_percent: None,
                roll: parse_roll(RFID).unwrap(),
                paper: Some(PaperInfo {
                    width_tenths: 250,
                    length_tenths: 500,
                    label_type: 1,
                }),
                catalog: None,
                notices: vec![],
            }),
            ..PrinterStatus::new(false, true)
        }
    }
    #[test]
    fn legacy_roll_uses_catalog_capacity_and_printer_usage() {
        let mut payload = vec![1; 8];
        payload.push(13);
        payload.extend_from_slice(b"6978061950197");
        payload.extend([1, b'X', 0, 180, 0, 15, 1]);
        let mut status = sized_status();
        let r = status.reading.as_mut().unwrap();
        r.paper = None;
        r.roll = parse_roll(&payload).unwrap();
        assert_eq!(r.roll.as_ref().unwrap().print_limit, 180);
        assert_eq!(r.capacity(), None);
        assert_eq!(r.remaining(), None);
        r.catalog = Some(
            crate::catalog::parse_metadata(
                include_bytes!("../tests/fixtures/roll-6978061950197.json"),
                "6978061950197",
            )
            .unwrap(),
        );
        assert_eq!(r.capacity(), Some(155));
        assert_eq!(r.remaining(), Some(140));
        assert_eq!(r.paper_info().unwrap().size(), Some(D101LabelSize::W12L40));
        assert_eq!(status.remaining_text(), "140 из 155 шт.");
        assert_eq!(
            SizeProposalTracker::default()
                .offer(&status, &LabelSpec::default())
                .unwrap()
                .size,
            D101LabelSize::W12L40
        );
        status.reading.as_mut().unwrap().roll.as_mut().unwrap().used = 16;
        assert_eq!(status.remaining_text(), "139 из 155 шт.");
        // A firmware that does report capacity takes precedence over catalog metadata.
        status
            .reading
            .as_mut()
            .unwrap()
            .roll
            .as_mut()
            .unwrap()
            .capacity = Some(160);
        assert_eq!(status.remaining_text(), "144 из 160 шт.");
    }
    #[test]
    fn paper_reply_uses_mm_fields_not_pixels_or_gap_and_rejects_bad_lengths() {
        let bytes = [
            0, 16, 1, 64, 1, 0, 20, 1, 144, 0, 112, 0, 140, 0, 0, 0, 0, 0,
        ];
        assert_eq!(
            parse_paper(&bytes).unwrap().unwrap().size(),
            Some(D101LabelSize::W14L40)
        );
        for n in 0..bytes.len() {
            if n == 4 {
                assert!(parse_paper(&bytes[..n]).unwrap().is_none());
            } else {
                assert!(parse_paper(&bytes[..n]).is_err());
            }
        }
        assert!(parse_paper(&[0; 18]).is_err());
        let mut unknown = bytes;
        unknown[12] = 141;
        assert_eq!(parse_paper(&unknown).unwrap().unwrap().size(), None);
    }
    #[test]
    fn size_offer_is_once_per_roll_or_connection_not_per_poll_or_counter_change() {
        let mut tracker = SizeProposalTracker::default();
        let mut status = sized_status();
        let spec = LabelSpec::default();
        let proposal = tracker.offer(&status, &spec).unwrap();
        assert_eq!(proposal.size, D101LabelSize::W25L50);
        assert!(proposal.still_valid(&status));
        assert!(tracker.offer(&status, &spec).is_none()); // includes a declined proposal
        status.reading.as_mut().unwrap().roll.as_mut().unwrap().used += 1;
        assert!(tracker.offer(&status, &spec).is_none());
        status.reading.as_mut().unwrap().connection_id += 1;
        assert!(!proposal.still_valid(&status));
        assert!(tracker.offer(&status, &spec).is_some());
        status
            .reading
            .as_mut()
            .unwrap()
            .roll
            .as_mut()
            .unwrap()
            .barcode = "new roll".into();
        assert!(tracker.offer(&status, &spec).is_some());
        assert_eq!(status.remaining_text(), "Нет данных");
    }
    #[test]
    fn equal_unknown_stale_or_busy_media_never_offer_an_inferred_size() {
        let mut status = sized_status();
        let mut tracker = SizeProposalTracker::default();
        let mut same = LabelSpec::default();
        D101LabelSize::W25L50.apply(&mut same);
        assert!(tracker.offer(&status, &same).is_none());
        for phase in [
            StatusPhase::Unavailable,
            StatusPhase::Updating,
            StatusPhase::Simulation,
        ] {
            status.phase = phase;
            assert!(
                SizeProposalTracker::default()
                    .offer(&status, &LabelSpec::default())
                    .is_none()
            );
        }
        status.phase = StatusPhase::Available;
        status.printing = true;
        assert!(
            SizeProposalTracker::default()
                .offer(&status, &LabelSpec::default())
                .is_none()
        );
        status.printing = false;
        status
            .reading
            .as_mut()
            .unwrap()
            .paper
            .as_mut()
            .unwrap()
            .width_tenths = 240;
        assert!(
            SizeProposalTracker::default()
                .offer(&status, &LabelSpec::default())
                .is_none()
        );
        status.reading.as_mut().unwrap().paper = None;
        assert!(
            SizeProposalTracker::default()
                .offer(&status, &LabelSpec::default())
                .is_none()
        );
    }
}
