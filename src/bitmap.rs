use crate::i18n::tr;
use crate::protocol::Packet;
use ab_glyph::{Font, FontArc, PxScale, ScaleFont, point};
use anyhow::{Context, Result, ensure};
use image::{GrayImage, Luma};
use serde::{Deserialize, Serialize};

pub const HEAD_PIXELS: u32 = 192;
pub const DPI: f32 = 203.0;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Bitmap {
    pub width: u32,
    pub height: u32,
    /// MSB first; 1 is black, each row padded with white to a byte boundary.
    pub bits: Vec<u8>,
}

impl Bitmap {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            (1..=HEAD_PIXELS).contains(&self.width),
            tr("Ширина печати должна быть 1–192 точки")
        );
        ensure!(
            (1..=1600).contains(&self.height),
            tr("Длина этикетки должна быть 1–1600 точек")
        );
        ensure!(
            self.bits.len() == (self.width.div_ceil(8) * self.height) as usize,
            tr("Неверная длина растра")
        );
        Ok(())
    }
    pub fn from_gray(image: &GrayImage) -> Result<Self> {
        let (width, height) = image.dimensions();
        ensure!(
            (1..=HEAD_PIXELS).contains(&width) && (1..=1600).contains(&height),
            tr("Изображение не помещается в область D101")
        );
        let stride = width.div_ceil(8);
        let mut bits = vec![0; (stride * height) as usize];
        for (x, y, pixel) in image.enumerate_pixels() {
            if pixel[0] < 128 {
                bits[(y * stride + x / 8) as usize] |= 0x80 >> (x % 8);
            }
        }
        Ok(Self {
            width,
            height,
            bits,
        })
    }
    pub fn to_gray(&self) -> GrayImage {
        GrayImage::from_fn(self.width, self.height, |x, y| {
            Luma([
                if self.bits[(y * self.width.div_ceil(8) + x / 8) as usize] & (0x80 >> (x % 8)) != 0
                {
                    0
                } else {
                    255
                },
            ])
        })
    }
    /// Shift across the physical printhead, after page orientation has been applied.
    /// Positive columns correspond to down on a normally oriented editor label.
    pub(crate) fn shift_head_columns(&mut self, dots: i8) -> Result<()> {
        if dots == 0 {
            return Ok(());
        }
        self.validate()?;
        let stride = self.width.div_ceil(8) as usize;
        let tail_mask = 0xffu8 << ((8 - self.width % 8) % 8);
        let mut source = [0u8; (HEAD_PIXELS / 8) as usize];
        for row in self.bits.chunks_exact_mut(stride) {
            source[..stride].copy_from_slice(row);
            source[stride - 1] &= tail_mask;
            let input = &source[..stride];
            for (byte, value) in row.iter_mut().enumerate() {
                let bit = byte as i32 * 8 - i32::from(dots);
                let first = bit.div_euclid(8);
                let pair = u16::from_be_bytes([
                    input.get(first as usize).copied().unwrap_or(0),
                    input.get((first + 1) as usize).copied().unwrap_or(0),
                ]);
                *value = (pair >> (8 - bit.rem_euclid(8))) as u8;
            }
            row[stride - 1] &= tail_mask;
        }
        Ok(())
    }
    pub fn row_packets(&self) -> Result<Vec<Packet>> {
        self.validate()?;
        let stride = self.width.div_ceil(8) as usize;
        let mut packets = Vec::new();
        let mut row = 0;
        while row < self.height as usize {
            let data = &self.bits[row * stride..(row + 1) * stride];
            let mut repeats = 1;
            while repeats < 255
                && row + repeats < self.height as usize
                && &self.bits[(row + repeats) * stride..(row + repeats + 1) * stride] == data
            {
                repeats += 1;
            }
            let mut payload = (row as u16).to_be_bytes().to_vec();
            let count: u16 = data.iter().map(|b| b.count_ones() as u16).sum();
            if count == 0 {
                payload.push(repeats as u8);
                packets.push(Packet::new(0x84, payload));
            } else {
                // Three 64-dot sections of the 192-dot D101 printhead.
                let mut counts = [0u8; 3];
                for (i, byte) in data.iter().enumerate() {
                    counts[i / 8] += byte.count_ones() as u8;
                }
                payload.extend_from_slice(&counts);
                payload.push(repeats as u8);
                payload.extend_from_slice(data);
                packets.push(Packet::new(0x85, payload));
            }
            row += repeats;
        }
        Ok(packets)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LabelSpec {
    #[serde(default)]
    pub outline: crate::label_outline::LabelOutline,
    /// Readable label orientation: length horizontally, roll width vertically.
    pub length_mm: f32,
    pub width_mm: f32,
    pub font_px: f32,
    pub rotate_180: bool,
}
impl Default for LabelSpec {
    fn default() -> Self {
        Self {
            outline: Default::default(),
            length_mm: 40.0,
            width_mm: 14.0,
            font_px: 28.0,
            rotate_180: false,
        }
    }
}
impl LabelSpec {
    pub fn dimensions(&self) -> Result<(u32, u32)> {
        ensure!(
            self.length_mm.is_finite() && (10.0..=200.0).contains(&self.length_mm),
            tr("Длина: 10–200 мм")
        );
        ensure!(
            self.width_mm.is_finite() && (6.0..=25.0).contains(&self.width_mm),
            tr("Ширина рулона: до 25 мм")
        );
        ensure!(
            self.font_px.is_finite() && (8.0..=150.0).contains(&self.font_px),
            tr("Размер текста: 8–150 точек")
        );
        Ok((
            (self.length_mm / 25.4 * DPI).round() as u32,
            (self.width_mm / 25.4 * DPI).round() as u32,
        ))
    }
    pub fn supported_size(&self) -> Result<crate::label_size::D101LabelSize> {
        crate::label_size::D101LabelSize::from_spec(self).context(tr(
            "Выберите поддерживаемый размер D101 в меню «Настройки → Размер этикетки»",
        ))
    }
    pub fn mask(&self) -> Result<GrayImage> {
        self.outline
            .mask(self.length_mm, self.width_mm, self.dimensions()?)
    }
}

/// Preserve physical scale. A 25 mm roll has non-printable margins around the 24 mm head.
pub fn crop_to_head(raster: &GrayImage) -> GrayImage {
    let width = raster.width().min(HEAD_PIXELS);
    image::imageops::crop_imm(
        raster,
        (raster.width() - width) / 2,
        0,
        width,
        raster.height(),
    )
    .to_image()
}

pub fn system_font() -> Result<FontArc> {
    let win = std::env::var_os("WINDIR").unwrap_or_else(|| "C:\\Windows".into());
    let bytes = std::fs::read(std::path::Path::new(&win).join("Fonts/segoeui.ttf"))
        .context(tr("Не найден шрифт Segoe UI"))?;
    FontArc::try_from_vec(bytes).context(tr("Не удалось прочитать шрифт"))
}

pub fn render_text(text: &str, spec: &LabelSpec, font: &FontArc) -> Result<Bitmap> {
    spec.supported_size()?;
    ensure!(!text.trim().is_empty(), tr("Введите текст этикетки"));
    ensure!(
        text.chars().count() <= 4096,
        tr("Слишком длинный текст этикетки")
    );
    let (width, height) = spec.dimensions()?;
    let mut canvas = GrayImage::from_pixel(width, height, Luma([255]));
    let lines: Vec<_> = text.lines().collect();
    let margin = 6.0;
    let mut size = spec.font_px;
    loop {
        let scaled = font.as_scaled(PxScale::from(size));
        let widest = lines
            .iter()
            .map(|line| text_width(line, &scaled))
            .fold(0.0, f32::max);
        if widest <= width as f32 - 2.0 * margin
            && lines.len() as f32 * scaled.height() <= height as f32 - 2.0 * margin
        {
            break;
        }
        size -= 1.0;
        ensure!(
            size >= 8.0,
            tr("Текст не помещается: сократите текст или увеличьте этикетку")
        );
    }
    let scaled = font.as_scaled(PxScale::from(size));
    let top = (height as f32 - lines.len() as f32 * scaled.height()) / 2.0;
    for (n, line) in lines.iter().enumerate() {
        let mut x = (width as f32 - text_width(line, &scaled)) / 2.0;
        let mut prev = None;
        for ch in line.chars() {
            let id = scaled.glyph_id(ch);
            if let Some(p) = prev {
                x += scaled.kern(p, id);
            }
            let glyph = id.with_scale_and_position(
                size,
                point(x, top + n as f32 * scaled.height() + scaled.ascent()),
            );
            if let Some(g) = font.outline_glyph(glyph) {
                let bounds = g.px_bounds();
                g.draw(|gx, gy, v| {
                    let px = bounds.min.x as i32 + gx as i32;
                    let py = bounds.min.y as i32 + gy as i32;
                    if px >= 0 && py >= 0 && px < width as i32 && py < height as i32 {
                        let pixel = canvas.get_pixel_mut(px as u32, py as u32);
                        pixel[0] = pixel[0].min((255.0 * (1.0 - v)) as u8);
                    }
                });
            }
            x += scaled.h_advance(id);
            prev = Some(id);
        }
    }
    for (pixel, mask) in canvas.pixels_mut().zip(spec.mask()?.pixels()) {
        if mask[0] < 128 {
            pixel[0] = 255;
        }
    }
    // D101 feeds left: readable horizontal label -> printhead columns.
    let raster = crop_to_head(&image::imageops::rotate270(&canvas));
    let raster = if spec.rotate_180 {
        image::imageops::rotate180(&raster)
    } else {
        raster
    };
    Bitmap::from_gray(&raster)
}

fn text_width<F: Font>(line: &str, font: &impl ScaleFont<F>) -> f32 {
    let mut width = 0.0;
    let mut prev = None;
    for ch in line.chars() {
        let id = font.glyph_id(ch);
        if let Some(p) = prev {
            width += font.kern(p, id);
        }
        width += font.h_advance(id);
        prev = Some(id);
    }
    width
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn head_offset_moves_across_rows_and_clips_even_at_non_byte_widths() {
        let raw = GrayImage::from_fn(19, 5, |x, y| {
            Luma([if (x + y * 3) % 5 == 0 { 0 } else { 255 }])
        });
        for dots in -16_i8..=16 {
            let mut page = Bitmap::from_gray(&raw).unwrap();
            page.shift_head_columns(dots).unwrap();
            let expected = GrayImage::from_fn(19, 5, |x, y| {
                let old = x as i32 - i32::from(dots);
                if (0..19).contains(&old) {
                    *raw.get_pixel(old as u32, y)
                } else {
                    Luma([255])
                }
            });
            assert_eq!(page.bits, Bitmap::from_gray(&expected).unwrap().bits);
        }
    }
    #[test]
    fn raster_bit_order_and_row_repeat() {
        let img = GrayImage::from_fn(9, 2, |x, _| Luma([if x == 0 || x == 8 { 0 } else { 255 }]));
        let b = Bitmap::from_gray(&img).unwrap();
        assert_eq!(b.bits, [128, 128, 128, 128]);
        assert_eq!(
            b.row_packets().unwrap(),
            [Packet::new(0x85, [0, 0, 2, 0, 0, 2, 128, 128])]
        );
    }
    #[test]
    fn blank_runs_split_at_255() {
        let b = Bitmap::from_gray(&GrayImage::from_pixel(192, 300, Luma([255]))).unwrap();
        assert_eq!(
            b.row_packets().unwrap(),
            [
                Packet::new(0x84, [0, 0, 255]),
                Packet::new(0x84, [0, 255, 45])
            ]
        );
    }
}
