//! Physical die-cut mask, shared by canvas, PNG export and editor print raster.
use crate::i18n::tr;
use crate::label_size::D101LabelSize;
use anyhow::{Result, ensure};
use image::{GrayImage, Luma};
use serde::{Deserialize, Serialize};
use std::io::Cursor;

pub const STOCK_12X40: &[u8] = include_bytes!("../assets/labels/6978061950197.png");
// Readable orientation of the D101 printout, in millimetres from the left tip.
pub const CABLE_WRAP_MM: f32 = 35.0;
pub const CABLE_FACE_MM: f32 = 37.0;
pub const CABLE_FOLD_MM: f32 = CABLE_WRAP_MM + CABLE_FACE_MM;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum LabelOutline {
    #[default]
    Auto,
    Rectangle,
    Rounded {
        radius_tenths: u16,
    },
    Ellipse,
    CableFlag,
    Catalog {
        size: D101LabelSize,
        alpha_png: Vec<u8>,
    },
}
impl LabelOutline {
    pub fn title(&self) -> &'static str {
        match self {
            Self::Auto => tr("По формату"),
            Self::Rectangle => tr("Прямоугольная"),
            Self::Rounded { .. } => tr("Скруглённая"),
            Self::Ellipse => tr("Круглая / овальная"),
            Self::CableFlag => tr("Кабельная 74 + 35 мм"),
            Self::Catalog { .. } => tr("Контур рулона NIIMBOT"),
        }
    }
    pub fn from_template(bytes: &[u8], size: D101LabelSize) -> Result<Self> {
        ensure!(bytes.len() <= 2_097_152, tr("Шаблон рулона больше 2 МБ"));
        let mut reader = image::ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(4096);
        limits.max_image_height = Some(4096);
        limits.max_alloc = Some(64 * 1024 * 1024);
        reader.limits(limits);
        let img = reader.decode()?;
        ensure!(
            img.color().has_alpha(),
            tr("Шаблон не содержит прозрачного контура")
        );
        let mut rgba = img.to_rgba8();
        let (across, feed) = size.tenths();
        let ratio = feed as f64 / across as f64;
        let actual = rgba.width() as f64 / rgba.height() as f64;
        if (actual / ratio - 1.0).abs() > 0.03 {
            ensure!(
                (1.0 / actual / ratio - 1.0).abs() <= 0.03,
                tr("Контур не соответствует пропорциям этикетки")
            );
            rgba = image::imageops::rotate90(&rgba);
        }
        // Ignore printed artwork, colour and decoration: only the alpha channel defines paper.
        let mask = GrayImage::from_fn(rgba.width(), rgba.height(), |x, y| {
            Luma([rgba.get_pixel(x, y)[3]])
        });
        let (w, h) = stock_pixels(size);
        let mask = image::imageops::resize(&mask, w, h, image::imageops::FilterType::Triangle);
        ensure!(
            mask.pixels().any(|p| p[0] >= 128),
            tr("Пустой контур этикетки")
        );
        let mut out = Cursor::new(Vec::new());
        mask.write_to(&mut out, image::ImageFormat::Png)?;
        Ok(Self::Catalog {
            size,
            alpha_png: out.into_inner(),
        })
    }
    pub fn mask(&self, length: f32, width: f32, pixels: (u32, u32)) -> Result<GrayImage> {
        let (w, h) = pixels;
        if matches!(self, Self::CableFlag) {
            ensure!(
                Self::is_cable_size(length, width),
                tr("Кабельный контур рассчитан на 12,5 × 109 мм")
            );
        }
        if let Self::Catalog { size, alpha_png } = self {
            let (sw, sl) = size.tenths();
            ensure!(
                (width - sw as f32 / 10.0).abs() < 0.001
                    && (length - sl as f32 / 10.0).abs() < 0.001,
                tr("Контур относится к другому размеру этикетки")
            );
            ensure!(
                alpha_png.len() <= 131_072,
                tr("Слишком большой контур этикетки")
            );
            let mut reader =
                image::ImageReader::new(Cursor::new(alpha_png)).with_guessed_format()?;
            let mut limits = image::Limits::default();
            limits.max_image_width = Some(1600);
            limits.max_image_height = Some(200);
            limits.max_alloc = Some(4 * 1024 * 1024);
            reader.limits(limits);
            let mask = reader.decode()?.to_luma8();
            ensure!(
                mask.dimensions() == pixels,
                tr("Некорректный растр контура")
            );
            return Ok(mask);
        }
        if *self == Self::Auto && (width - 12.0).abs() < 0.001 && (length - 40.0).abs() < 0.001 {
            return Self::from_template(STOCK_12X40, D101LabelSize::W12L40)?
                .mask(length, width, pixels);
        }
        let circular = matches!(self, Self::Ellipse)
            || (*self == Self::Auto && (width == 20.0 || width == 25.0) && length == width);
        let radius = match self {
            Self::Auto => 2.0,
            Self::Rounded { radius_tenths } => *radius_tenths as f32 / 10.0,
            _ => 0.0,
        }
        .min(width.min(length) / 2.0);
        let cable = self.is_cable(length, width);
        Ok(GrayImage::from_fn(w, h, |x, y| {
            // Supersampling makes the preview smooth; printing uses the same 128 threshold.
            let mut inside = 0u16;
            for sy in 0..4 {
                for sx in 0..4 {
                    let px = (x as f32 + (sx as f32 + 0.5) / 4.0) / w as f32 * length;
                    let py = (y as f32 + (sy as f32 + 0.5) / 4.0) / h as f32 * width;
                    let hit = if cable {
                        rounded_contains(px, py, 0.0, 3.125, CABLE_WRAP_MM + 1.0, 9.375, 0.8)
                            || rounded_contains(
                                px,
                                py,
                                CABLE_WRAP_MM,
                                0.0,
                                CABLE_FOLD_MM,
                                width,
                                0.8,
                            )
                            || rounded_contains(px, py, CABLE_FOLD_MM, 0.0, length, width, 0.8)
                    } else if circular {
                        ((px - length / 2.0) / (length / 2.0)).powi(2)
                            + ((py - width / 2.0) / (width / 2.0)).powi(2)
                            <= 1.0
                    } else if radius > 0.0 {
                        let cx = px.clamp(radius, length - radius);
                        let cy = py.clamp(radius, width - radius);
                        (px - cx).powi(2) + (py - cy).powi(2) <= radius.powi(2)
                    } else {
                        true
                    };
                    inside += u16::from(hit);
                }
            }
            Luma([(inside * 255 / 16) as u8])
        }))
    }
    fn is_cable_size(length: f32, width: f32) -> bool {
        (length - 109.0).abs() < 0.001 && (width - 12.5).abs() < 0.001
    }
    pub fn is_cable(&self, length: f32, width: f32) -> bool {
        matches!(self, Self::Auto | Self::CableFlag) && Self::is_cable_size(length, width)
    }
}
fn rounded_contains(x: f32, y: f32, left: f32, top: f32, right: f32, bottom: f32, r: f32) -> bool {
    x >= left
        && x <= right
        && y >= top
        && y <= bottom
        && (x - x.clamp(left + r, right - r)).powi(2) + (y - y.clamp(top + r, bottom - r)).powi(2)
            <= r * r
}
fn stock_pixels(size: D101LabelSize) -> (u32, u32) {
    let (w, l) = size.tenths();
    (
        (l as f32 / 10.0 / 25.4 * crate::bitmap::DPI).round() as u32,
        (w as f32 / 10.0 / 25.4 * crate::bitmap::DPI).round() as u32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cable_flag_has_two_faces_and_a_narrow_wrap_without_stretching_the_flag() {
        let pixels = stock_pixels(D101LabelSize::W12_5L109);
        let mask = LabelOutline::Auto.mask(109.0, 12.5, pixels).unwrap();
        let at = |x: f32, y: f32| {
            mask.get_pixel(
                (x / 109.0 * pixels.0 as f32) as u32,
                (y / 12.5 * pixels.1 as f32) as u32,
            )[0]
        };
        for x in [36.0, 71.0, 73.0, 107.0] {
            assert_eq!(at(x, 1.0), 255);
        }
        for x in [2.0, 18.0, 34.0] {
            assert_eq!(at(x, 1.0), 0);
            assert_eq!(at(x, 6.25), 255);
            assert_eq!(at(x, 11.5), 0);
        }
        assert_eq!(
            mask,
            LabelOutline::CableFlag.mask(109.0, 12.5, pixels).unwrap()
        );
        assert!(LabelOutline::CableFlag.mask(40.0, 12.0, (320, 96)).is_err());
    }
    #[test]
    fn template_keeps_transparent_cutouts_and_never_prints_catalog_artwork() {
        let rgba = image::RgbaImage::from_fn(100, 50, |x, y| {
            image::Rgba([
                0,
                0,
                0,
                if (40..60).contains(&x) && y < 15 {
                    0
                } else {
                    255
                },
            ])
        });
        let mut png = Cursor::new(Vec::new());
        rgba.write_to(&mut png, image::ImageFormat::Png).unwrap();
        let shape = LabelOutline::from_template(&png.into_inner(), D101LabelSize::W20L40).unwrap();
        let mask = shape
            .mask(40.0, 20.0, stock_pixels(D101LabelSize::W20L40))
            .unwrap();
        assert_eq!(mask.get_pixel(mask.width() / 2, 1)[0], 0);
        assert_eq!(mask.get_pixel(1, 1)[0], 255);
        assert!(
            shape
                .mask(30.0, 20.0, stock_pixels(D101LabelSize::W20L30))
                .is_err()
        );
    }
    #[test]
    fn actual_12x40_mask_has_rounded_corners_and_rejects_wrong_aspect_or_corruption() {
        let shape = LabelOutline::from_template(STOCK_12X40, D101LabelSize::W12L40).unwrap();
        let mask = shape
            .mask(40.0, 12.0, stock_pixels(D101LabelSize::W12L40))
            .unwrap();
        assert_eq!(mask.get_pixel(0, 0)[0], 0);
        assert!(mask.get_pixel(mask.width() / 2, mask.height() / 2)[0] >= 128);
        assert!(LabelOutline::from_template(STOCK_12X40, D101LabelSize::W20L20).is_err());
        assert!(LabelOutline::from_template(&[1, 2, 3], D101LabelSize::W12L40).is_err());
    }
}
