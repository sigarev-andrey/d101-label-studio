//! Editable labels use physical millimetres, independent of window DPI and zoom.
use crate::bitmap::{Bitmap, DPI, LabelSpec};
use crate::label_outline::{CABLE_FACE_MM, CABLE_WRAP_MM};
use crate::{i18n::tr, local_format};
use anyhow::{Context, Result, ensure};
use image::{GrayImage, Luma};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Axis {
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(u8)]
pub enum Alignment {
    Start,
    Center,
    End,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Bounds {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}
impl Bounds {
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && y >= self.y && x <= self.x + self.width && y <= self.y + self.height
    }
    pub fn clamp_to(&mut self, width: f32, height: f32) {
        self.width = self.width.clamp(0.5, width);
        self.height = self.height.clamp(0.5, height);
        self.x = self.x.clamp(0.0, (width - self.width).max(0.0));
        self.y = self.y.clamp(0.0, (height - self.height).max(0.0));
    }
    pub fn moved(&self, dx: f32, dy: f32, width: f32, height: f32, snap: bool) -> Self {
        let mut result = *self;
        result.x += dx;
        result.y += dy;
        if snap {
            result.x = (result.x * 2.0).round() / 2.0;
            result.y = (result.y * 2.0).round() / 2.0;
        }
        result.clamp_to(width, height);
        result
    }
    pub fn align_to(&mut self, axis: Axis, alignment: Alignment, width: f32, height: f32) {
        let (position, space) = match axis {
            Axis::Horizontal => (&mut self.x, width - self.width),
            Axis::Vertical => (&mut self.y, height - self.height),
        };
        // Alignment is exact, independently of the drag grid.
        *position = space.max(0.0)
            * match alignment {
                Alignment::Start => 0.0,
                Alignment::Center => 0.5,
                Alignment::End => 1.0,
            };
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TextStyle {
    pub family: String,
    pub points: f32,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikeout: bool,
    pub alignment: u8,
    pub vertical_alignment: u8,
}
impl Default for TextStyle {
    fn default() -> Self {
        Self {
            family: "Segoe UI".into(),
            points: 10.0,
            bold: false,
            italic: false,
            underline: false,
            strikeout: false,
            alignment: 0,
            vertical_alignment: 0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum Content {
    Text {
        text: String,
        style: TextStyle,
    },
    Image {
        name: String,
        png: Vec<u8>,
        threshold: u8,
    },
    Rectangle {
        stroke_mm: f32,
        filled: bool,
    },
    Line {
        stroke_mm: f32,
    },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Block {
    pub id: u64,
    pub bounds: Bounds,
    pub content: Content,
}
impl Block {
    pub fn title(&self) -> String {
        match &self.content {
            Content::Text { text, .. } => local_format!(
                "Текст: {}",
                "Text: {}",
                text.lines()
                    .next()
                    .unwrap_or("")
                    .chars()
                    .take(22)
                    .collect::<String>()
            ),
            Content::Image { name, .. } => local_format!("Изображение: {name}", "Image: {name}"),
            Content::Rectangle { .. } => tr("Прямоугольник").into(),
            Content::Line { .. } => tr("Линия").into(),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Label {
    pub name: String,
    pub copies: usize,
    pub enabled: bool,
    pub blocks: Vec<Block>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Document {
    pub version: u32,
    #[serde(default)]
    pub generator: crate::generator::Settings,
    pub spec: LabelSpec,
    pub labels: Vec<Label>,
}

impl Document {
    pub const VERSION: u32 = 3;

    pub fn new(spec: LabelSpec) -> Self {
        let mut doc = Self {
            version: Self::VERSION,
            generator: Default::default(),
            spec,
            labels: vec![Label {
                name: tr("Этикетка 1").into(),
                copies: 1,
                enabled: true,
                blocks: vec![],
            }],
        };
        doc.add_text(0, tr("Кабель USB-C").into());
        doc
    }
    pub fn next_id(&self) -> u64 {
        self.labels
            .iter()
            .flat_map(|p| &p.blocks)
            .map(|b| b.id)
            .max()
            .unwrap_or(0)
            + 1
    }
    pub fn add_text(&mut self, page: usize, text: String) -> u64 {
        let id = self.next_id();
        let bounds = self.initial_bounds(self.spec.length_mm - 4.0, self.spec.width_mm - 4.0);
        self.labels[page].blocks.push(Block {
            id,
            bounds,
            content: Content::Text {
                text,
                style: TextStyle::default(),
            },
        });
        id
    }
    pub fn initial_bounds(&self, width: f32, height: f32) -> Bounds {
        let cable = self
            .spec
            .outline
            .is_cable(self.spec.length_mm, self.spec.width_mm);
        let mut bounds = Bounds {
            x: 2.0 + if cable { CABLE_WRAP_MM } else { 0.0 },
            y: 2.0,
            width: if cable {
                width.min(CABLE_FACE_MM - 4.0)
            } else {
                width
            },
            height,
        };
        bounds.clamp_to(self.spec.length_mm, self.spec.width_mm);
        bounds
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.version == Self::VERSION,
            tr("Неизвестная версия файла редактора")
        );
        self.generator.validate()?;
        self.spec.dimensions()?;
        if matches!(
            self.spec.outline,
            crate::label_outline::LabelOutline::Catalog { .. }
                | crate::label_outline::LabelOutline::CableFlag
        ) {
            self.spec.mask()?;
        }
        ensure!(
            !self.labels.is_empty() && self.labels.len() <= 1000,
            tr("Нужно 1–1000 этикеток")
        );
        let mut ids = std::collections::HashSet::new();
        let mut image_bytes = 0;
        for page in &self.labels {
            ensure!(
                (1..=1000).contains(&page.copies) && page.blocks.len() <= 100,
                tr("Максимум 100 блоков и 1000 копий на этикетку")
            );
            for block in &page.blocks {
                ensure!(
                    ids.insert(block.id) && block.id < u64::MAX,
                    tr("Повторяющийся идентификатор блока")
                );
                let b = block.bounds;
                ensure!(
                    [b.x, b.y, b.width, b.height].iter().all(|n| n.is_finite())
                        && b.width >= 0.5
                        && b.height >= 0.5
                        && b.x >= 0.0
                        && b.y >= 0.0
                        && b.x + b.width <= self.spec.length_mm + 0.01
                        && b.y + b.height <= self.spec.width_mm + 0.01,
                    tr("Блок выходит за пределы этикетки")
                );
                match &block.content {
                    Content::Text { text, style } => {
                        ensure!(
                            text.chars().count() <= 4096
                                && style.family.len() <= 128
                                && style.points.is_finite()
                                && (1.0..=144.0).contains(&style.points)
                                && style.alignment <= 2
                                && style.vertical_alignment <= 2,
                            tr("Некорректные параметры текста")
                        );
                    }
                    Content::Image { png, .. } => {
                        image_bytes += png.len();
                        ensure!(
                            png.len() <= 4 * 1024 * 1024 && image_bytes <= 16 * 1024 * 1024,
                            tr("Изображения превышают лимит памяти")
                        );
                    }
                    Content::Rectangle { stroke_mm, .. } | Content::Line { stroke_mm } => ensure!(
                        stroke_mm.is_finite() && (0.1..=5.0).contains(stroke_mm),
                        tr("Толщина линии: 0,1–5 мм")
                    ),
                }
            }
        }
        Ok(())
    }
    pub fn hit_test(&self, page: usize, x: f32, y: f32) -> Option<u64> {
        if x < 0.0 || y < 0.0 || x >= self.spec.length_mm || y >= self.spec.width_mm {
            return None;
        }
        let mask = self.spec.mask().ok()?;
        let px = ((x / self.spec.length_mm * mask.width() as f32) as u32).min(mask.width() - 1);
        let py = ((y / self.spec.width_mm * mask.height() as f32) as u32).min(mask.height() - 1);
        if mask.get_pixel(px, py)[0] < 128 {
            return None;
        }
        self.labels[page]
            .blocks
            .iter()
            .rev()
            .find(|b| b.bounds.contains(x, y))
            .map(|b| b.id)
    }
    pub fn resize_label(&mut self, length: f32, width: f32) {
        if self.spec.length_mm != length || self.spec.width_mm != width {
            self.spec.outline = Default::default();
        }
        self.spec.length_mm = length;
        self.spec.width_mm = width;
        for p in &mut self.labels {
            for b in &mut p.blocks {
                b.bounds.clamp_to(length, width);
            }
        }
    }
    pub fn render(&self, page: usize) -> Result<GrayImage> {
        self.validate()?;
        let (width, height) = self.spec.dimensions()?;
        let mut out = GrayImage::from_pixel(width, height, Luma([255]));
        for block in &self
            .labels
            .get(page)
            .context(tr("Этикетка не найдена"))?
            .blocks
        {
            let b = block.bounds;
            let x = mm(b.x);
            let y = mm(b.y);
            let w = mm(b.width).max(1).min(width - x.min(width));
            let h = mm(b.height).max(1).min(height - y.min(height));
            if w == 0 || h == 0 {
                continue;
            }
            for (dx, dy, p) in render_block_content(&block.content, w, h)?.enumerate_pixels() {
                let pixel = out.get_pixel_mut(x + dx, y + dy);
                pixel[0] = pixel[0].min(p[0]);
            }
        }
        for (pixel, mask) in out.pixels_mut().zip(self.spec.mask()?.pixels()) {
            if mask[0] < 128 {
                pixel[0] = 255;
            }
        }
        Ok(out)
    }
    pub fn preview(&self, page: usize, mark_margins: bool) -> Result<image::RgbaImage> {
        let art = self.render(page)?;
        let mask = self.spec.mask()?;
        let margin = art.height().saturating_sub(crate::bitmap::HEAD_PIXELS) / 2;
        Ok(image::RgbaImage::from_fn(
            art.width(),
            art.height(),
            |x, y| {
                let gray = if mark_margins && (y < margin || y >= art.height() - margin) {
                    210
                } else {
                    art.get_pixel(x, y)[0]
                };
                image::Rgba([gray, gray, gray, mask.get_pixel(x, y)[0]])
            },
        ))
    }
    pub fn print_pages(&self) -> Result<Vec<Bitmap>> {
        self.spec.supported_size()?;
        let total: usize = self
            .labels
            .iter()
            .filter(|p| p.enabled)
            .map(|p| p.copies)
            .sum();
        ensure!(
            (1..=1000).contains(&total),
            tr("Выберите от 1 до 1000 экземпляров")
        );
        let mut pages = Vec::new();
        for (i, label) in self.labels.iter().enumerate().filter(|(_, p)| p.enabled) {
            let img = crate::bitmap::crop_to_head(&image::imageops::rotate270(&self.render(i)?));
            let img = if self.spec.rotate_180 {
                image::imageops::rotate180(&img)
            } else {
                img
            };
            let bitmap = Bitmap::from_gray(&img)?;
            pages.extend(std::iter::repeat_n(bitmap, label.copies));
        }
        Ok(pages)
    }
    pub fn load(path: &std::path::Path) -> Result<Self> {
        ensure!(
            std::fs::metadata(path)?.len() <= 64 * 1024 * 1024,
            tr("Файл проекта превышает 64 МБ")
        );
        let bytes = std::fs::read(path)?;
        Self::from_bytes(&bytes)
    }
    fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if let Ok(mut doc) = serde_json::from_slice::<Self>(bytes) {
            let legacy = doc.version == 2;
            if legacy {
                doc.version = Self::VERSION;
            }
            doc.validate()?;
            if legacy
                && doc
                    .spec
                    .outline
                    .is_cable(doc.spec.length_mm, doc.spec.width_mm)
            {
                let old_shoulder = 2.0 * CABLE_FACE_MM;
                for b in doc
                    .labels
                    .iter_mut()
                    .flat_map(|p| &mut p.blocks)
                    .map(|b| &mut b.bounds)
                {
                    b.x = if b.x + b.width <= old_shoulder + 0.001 {
                        b.x + CABLE_WRAP_MM
                    } else if b.x >= old_shoulder {
                        b.x - old_shoulder
                    } else {
                        // A single block spans the old shoulder. Keep the overlap
                        // on either side of the new shoulder, without mirroring its art.
                        doc.spec.length_mm - b.x - b.width
                    }
                    .clamp(0.0, (doc.spec.length_mm - b.width).max(0.0));
                }
            }
            return Ok(doc);
        }
        #[derive(Deserialize)]
        struct OldRow {
            text: String,
            copies: usize,
            enabled: bool,
        }
        #[derive(Deserialize)]
        struct Old {
            label: LabelSpec,
            rows: Vec<OldRow>,
        }
        let old: Old =
            serde_json::from_slice(bytes).context(tr("Неверный файл набора этикеток"))?;
        let mut doc = Self {
            version: Self::VERSION,
            generator: Default::default(),
            spec: old.label,
            labels: Vec::new(),
        };
        doc.spec.dimensions()?;
        for (i, row) in old.rows.into_iter().enumerate() {
            doc.labels.push(Label {
                name: local_format!("Этикетка {}", "Label {}", i + 1),
                copies: row.copies,
                enabled: row.enabled,
                blocks: vec![],
            });
            doc.add_text(i, row.text);
        }
        doc.validate()?;
        Ok(doc)
    }
}
pub fn mm(value: f32) -> u32 {
    (value * DPI / 25.4).round() as u32
}
/// Shared by the cached editor layers and the final print raster.
pub fn render_block_content(content: &Content, w: u32, h: u32) -> Result<GrayImage> {
    ensure!(
        (1..=1600).contains(&w) && (1..=200).contains(&h),
        tr("Некорректный размер блока")
    );
    let mut out = GrayImage::from_pixel(w, h, Luma([255]));
    match content {
        Content::Text { text, style } => return crate::win_text::render(text, style, w, h),
        Content::Image { png, threshold, .. } => {
            let image = image::imageops::thumbnail(&load_image(png)?, w, h);
            let (ox, oy) = ((w - image.width()) / 2, (h - image.height()) / 2);
            for (dx, dy, p) in image.enumerate_pixels() {
                if p[0] < *threshold {
                    out.put_pixel(ox + dx, oy + dy, Luma([0]));
                }
            }
        }
        Content::Rectangle { stroke_mm, filled } => {
            let s = mm(*stroke_mm).max(1);
            for y in 0..h {
                for x in 0..w {
                    if *filled || x < s || y < s || x + s >= w || y + s >= h {
                        out.put_pixel(x, y, Luma([0]));
                    }
                }
            }
        }
        Content::Line { stroke_mm } => {
            let s = mm(*stroke_mm).max(1).min(h);
            for y in 0..s {
                for x in 0..w {
                    out.put_pixel(x, (h - s) / 2 + y, Luma([0]));
                }
            }
        }
    }
    Ok(out)
}
pub fn load_image(data: &[u8]) -> Result<GrayImage> {
    let mut reader = image::ImageReader::new(std::io::Cursor::new(data)).with_guessed_format()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let rgba = reader.decode()?.to_rgba8();
    Ok(GrayImage::from_fn(rgba.width(), rgba.height(), |x, y| {
        let p = rgba.get_pixel(x, y);
        let gray = (u32::from(p[0]) * 299 + u32::from(p[1]) * 587 + u32::from(p[2]) * 114) / 1000;
        let a = u32::from(p[3]);
        Luma([((gray * a + 255 * (255 - a)) / 255) as u8])
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn old_cable_faces_move_once_to_the_two_physical_panels() {
        let mut spec = LabelSpec::default();
        crate::label_size::D101LabelSize::W12_5L109.apply(&mut spec);
        for outline in [
            crate::label_outline::LabelOutline::Auto,
            crate::label_outline::LabelOutline::CableFlag,
        ] {
            spec.outline = outline;
            let mut legacy = Document::new(spec.clone());
            assert_eq!(legacy.labels[0].blocks[0].bounds.x, 37.0);
            legacy.version = 2;
            // Two different marks, plus a small one on the narrow wrap.
            legacy.labels[0].blocks = [(0.0, 37.0), (37.0, 18.0), (77.0, 3.0)]
                .into_iter()
                .enumerate()
                .map(|(idx, (x, width))| Block {
                    id: idx as u64 + 1,
                    bounds: Bounds {
                        x,
                        width,
                        y: 5.0,
                        height: 2.0,
                    },
                    content: Content::Rectangle {
                        stroke_mm: 0.5,
                        filled: true,
                    },
                })
                .collect();
            let doc = Document::from_bytes(&serde_json::to_vec(&legacy).unwrap()).unwrap();
            assert_eq!(
                doc.labels[0]
                    .blocks
                    .iter()
                    .map(|b| b.bounds.x)
                    .collect::<Vec<_>>(),
                [35.0, 72.0, 3.0]
            );
            let restored = Document::from_bytes(&serde_json::to_vec(&doc).unwrap()).unwrap();
            assert_eq!(restored, doc);
            let bitmap = &doc.print_pages().unwrap()[0];
            let readable = image::imageops::rotate90(&bitmap.to_gray());
            for (x, value) in [(4.0, 0), (15.0, 255), (36.0, 0), (80.0, 0), (95.0, 255)] {
                assert_eq!(readable.get_pixel(mm(x), mm(6.0))[0], value);
            }
        }
    }
    #[test]
    fn die_cut_clips_preview_export_and_print_and_survives_project_round_trip() {
        let mut spec = LabelSpec::default();
        crate::label_size::D101LabelSize::W12L40.apply(&mut spec);
        spec.outline = crate::label_outline::LabelOutline::from_template(
            crate::label_outline::STOCK_12X40,
            crate::label_size::D101LabelSize::W12L40,
        )
        .unwrap();
        let mut doc = Document::new(spec);
        doc.labels[0].blocks = vec![Block {
            id: 1,
            bounds: Bounds {
                x: 0.0,
                y: 0.0,
                width: 40.0,
                height: 12.0,
            },
            content: Content::Rectangle {
                stroke_mm: 1.0,
                filled: true,
            },
        }];
        let loaded: Document = serde_json::from_slice(&serde_json::to_vec(&doc).unwrap()).unwrap();
        let preview = loaded.preview(0, false).unwrap();
        assert_eq!(preview.get_pixel(0, 0)[3], 0);
        assert_eq!(
            preview.get_pixel(preview.width() / 2, preview.height() / 2)[0],
            0
        );
        assert_eq!(loaded.hit_test(0, 0.0, 0.0), None);
        assert_eq!(loaded.hit_test(0, 20.0, 6.0), Some(1));
        let expected = image::imageops::rotate270(&loaded.render(0).unwrap());
        assert_eq!(loaded.print_pages().unwrap()[0].to_gray(), expected);
        assert_eq!(expected.get_pixel(0, 0)[0], 255);
        let mut resized = loaded;
        resized.resize_label(30.0, 20.0);
        assert_eq!(
            resized.spec.outline,
            crate::label_outline::LabelOutline::Auto
        );
        assert!(resized.validate().is_ok());
    }
    #[test]
    fn round_template_uses_circle_not_square_and_legacy_shape_defaults_load() {
        let mut spec = LabelSpec::default();
        crate::label_size::D101LabelSize::W20L20.apply(&mut spec);
        let doc = Document::new(spec);
        let preview = doc.preview(0, false).unwrap();
        assert_eq!(preview.get_pixel(0, 0)[3], 0);
        assert!(preview.get_pixel(preview.width() / 2, 0)[3] > 127);
        let mut json = serde_json::to_value(&doc).unwrap();
        json["spec"].as_object_mut().unwrap().remove("outline");
        let old: Document = serde_json::from_value(json).unwrap();
        assert_eq!(old.spec.outline, crate::label_outline::LabelOutline::Auto);
        assert!(old.validate().is_ok());
    }
    #[test]
    fn canvas_alignment_preserves_size_and_other_axis_without_snapping() {
        let original = Bounds {
            x: 3.0,
            y: 2.0,
            width: 13.5,
            height: 5.5,
        };
        for (alignment, x, y) in [
            (Alignment::Start, 0.0, 0.0),
            (Alignment::Center, 13.25, 4.25),
            (Alignment::End, 26.5, 8.5),
        ] {
            let mut b = original;
            b.align_to(Axis::Horizontal, alignment, 40.0, 14.0);
            assert_eq!(b, Bounds { x, ..original });
            b = original;
            b.align_to(Axis::Vertical, alignment, 40.0, 14.0);
            assert_eq!(b, Bounds { y, ..original });
        }
    }
    #[test]
    fn text_alignment_loads_old_projects_and_round_trips_both_axes() {
        let mut old = serde_json::to_value(Document::new(LabelSpec::default())).unwrap();
        let style = &mut old["labels"][0]["blocks"][0]["content"]["style"];
        style.as_object_mut().unwrap().remove("vertical_alignment");
        style["alignment"] = 2.into();
        let mut doc: Document = serde_json::from_value(old).unwrap();
        let Content::Text { style, .. } = &mut doc.labels[0].blocks[0].content else {
            panic!()
        };
        assert_eq!((style.alignment, style.vertical_alignment), (2, 0));
        style.vertical_alignment = 2;
        let restored: Document =
            serde_json::from_slice(&serde_json::to_vec(&doc).unwrap()).unwrap();
        assert_eq!(restored, doc);
        assert!(restored.validate().is_ok());
        let Content::Text { style, .. } = &mut doc.labels[0].blocks[0].content else {
            panic!()
        };
        style.vertical_alignment = 3;
        assert!(doc.validate().is_err());
    }
    #[test]
    fn dragging_clamps_and_snaps() {
        let b = Bounds {
            x: 1.0,
            y: 1.0,
            width: 10.0,
            height: 5.0,
        };
        let n = b.moved(100.0, 100.0, 40.0, 14.0, true);
        assert_eq!((n.x, n.y), (30.0, 9.0));
        assert_eq!(b.moved(0.3, 0.1, 40.0, 14.0, true).x, 1.5);
    }
    #[test]
    fn hit_test_uses_frontmost_block() {
        let mut d = Document::new(LabelSpec::default());
        let id = d.add_text(0, "top".into());
        assert_eq!(d.hit_test(0, 4.0, 4.0), Some(id));
        assert_eq!(d.hit_test(0, 0.0, 0.0), None);
    }
    #[test]
    fn changing_label_size_keeps_blocks_inside() {
        let mut d = Document::new(LabelSpec::default());
        d.resize_label(20.0, 8.0);
        assert!(d.validate().is_ok());
    }
    #[test]
    fn every_catalog_size_renders_to_d101_head_and_old_custom_documents_stay_readable() {
        for &size in crate::label_size::D101LabelSize::ALL {
            let mut spec = LabelSpec::default();
            size.apply(&mut spec);
            let d = Document::new(spec);
            let pages = d.print_pages().unwrap();
            assert_eq!(pages[0].width, mm(d.spec.width_mm).min(192));
            assert_eq!(pages[0].height, mm(d.spec.length_mm));
        }
        let mut legacy = Document::new(LabelSpec::default());
        legacy.resize_label(41.0, 14.0);
        assert!(legacy.render(0).is_ok());
        assert!(legacy.print_pages().is_err());
    }
    #[test]
    fn physical_text_style_changes_raster() {
        let mut d = Document::new(LabelSpec::default());
        let normal = d.render(0).unwrap();
        if let Content::Text { style, .. } = &mut d.labels[0].blocks[0].content {
            style.bold = true;
            style.italic = true;
            style.underline = true;
        }
        assert_ne!(normal, d.render(0).unwrap());
        assert_eq!(d.print_pages().unwrap()[0].width, 112);
    }
    #[test]
    fn embedded_image_round_trip_and_batch_orientation() {
        let mut d = Document::new(LabelSpec::default());
        d.labels[0].blocks.clear();
        let mut png = std::io::Cursor::new(Vec::new());
        GrayImage::from_pixel(8, 8, Luma([0]))
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        d.labels[0].blocks.push(Block {
            id: 1,
            bounds: Bounds {
                x: 1.0,
                y: 2.0,
                width: 3.0,
                height: 3.0,
            },
            content: Content::Image {
                name: "square.png".into(),
                png: png.into_inner(),
                threshold: 128,
            },
        });
        d.labels[0].copies = 3;
        let restored: Document = serde_json::from_slice(&serde_json::to_vec(&d).unwrap()).unwrap();
        assert_eq!(d, restored);
        let image = restored.render(0).unwrap();
        assert_eq!(image.get_pixel(mm(2.0), mm(3.0))[0], 0);
        assert_eq!(image.get_pixel(mm(10.0), mm(3.0))[0], 255);
        let pages = restored.print_pages().unwrap();
        assert_eq!(pages.len(), 3);
        let expected = Bitmap::from_gray(&image::imageops::rotate270(&image)).unwrap();
        assert!(pages.iter().all(|p| p.bits == expected.bits));
    }
}
