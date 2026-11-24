//! Cached block textures. Pointer movement changes native Canvas coordinates only.
use anyhow::Result;
use image::{Rgba, RgbaImage};
use niimbot_d101::{
    bitmap::{HEAD_PIXELS, LabelSpec},
    document::{Content, Document, mm, render_block_content},
};
use std::{collections::HashMap, io::Cursor};
use windows_reactor::EncodedImage;

pub const WORKSPACE_COLOR: [u8; 3] = [230, 232, 236];

#[derive(Clone)]
struct Layer {
    content: Content,
    size: (u32, u32),
    image: EncodedImage,
}

#[derive(Default)]
pub struct Scene {
    spec: Option<LabelSpec>,
    pub contour: Option<EncodedImage>,
    blocks: HashMap<u64, Layer>,
}
impl Scene {
    pub fn refresh(&mut self, doc: &Document, page: usize) -> Result<()> {
        doc.validate()?;
        let (w, h) = doc.spec.dimensions()?;
        let contour = if self.spec.as_ref() == Some(&doc.spec) {
            self.contour.clone()
        } else {
            Some(encode(&contour(&doc.spec)?)?)
        };
        let mut blocks = HashMap::new();
        for block in &doc.labels[page].blocks {
            let size = (
                mm(block.bounds.width)
                    .max(1)
                    .min(w - mm(block.bounds.x).min(w)),
                mm(block.bounds.height)
                    .max(1)
                    .min(h - mm(block.bounds.y).min(h)),
            );
            if size.0 == 0 || size.1 == 0 {
                continue;
            }
            let layer = if let Some(old) = self.blocks.get(&block.id)
                && old.content == block.content
                && old.size == size
            {
                old.clone()
            } else {
                let raster = render_block_content(&block.content, size.0, size.1)?;
                let rgba = RgbaImage::from_fn(size.0, size.1, |x, y| {
                    Rgba([0, 0, 0, 255 - raster.get_pixel(x, y)[0]])
                });
                Layer {
                    content: block.content.clone(),
                    size,
                    image: encode(&rgba)?,
                }
            };
            blocks.insert(block.id, layer);
        }
        self.blocks = blocks;
        self.contour = contour;
        self.spec = Some(doc.spec.clone());
        Ok(())
    }
    pub fn block_image(&self, id: u64) -> Option<&EncodedImage> {
        self.blocks.get(&id).map(|b| &b.image)
    }
}

fn encode(image: &RgbaImage) -> Result<EncodedImage> {
    let mut png = Cursor::new(Vec::new());
    image.write_to(&mut png, image::ImageFormat::Png)?;
    Ok(EncodedImage::new(png.into_inner()))
}

/// A stationary top layer clips every moving block to the paper's die-cut outline.
fn contour(spec: &LabelSpec) -> Result<RgbaImage> {
    let mask = spec.mask()?;
    let margin = mask.height().saturating_sub(HEAD_PIXELS) / 2;
    Ok(RgbaImage::from_fn(mask.width(), mask.height(), |x, y| {
        let a = mask.get_pixel(x, y)[0];
        let outside_head = y < margin || y >= mask.height() - margin;
        if a < 128 || outside_head {
            // Match Document::preview, including the printhead margin and antialiasing.
            let paper = if outside_head { 210u16 } else { 255 };
            let rgb = WORKSPACE_COLOR
                .map(|c| ((paper * a as u16 + c as u16 * (255 - a as u16)) / 255) as u8);
            Rgba([rgb[0], rgb[1], rgb[2], 255])
        } else {
            Rgba([
                WORKSPACE_COLOR[0],
                WORKSPACE_COLOR[1],
                WORKSPACE_COLOR[2],
                255 - a,
            ])
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use niimbot_d101::{
        document::{Block, Bounds},
        label_size::D101LabelSize,
    };

    #[test]
    fn moving_block_reuses_text_and_contour_bytes_but_editing_invalidates_only_text() {
        let mut doc = Document::new(LabelSpec::default());
        doc.labels[0].blocks[0].bounds.width = 15.0;
        let mut scene = Scene::default();
        scene.refresh(&doc, 0).unwrap();
        let text = scene.block_image(1).unwrap().clone();
        let shape = scene.contour.clone().unwrap();
        for x in 0..10 {
            doc.labels[0].blocks[0].bounds.x = x as f32;
            scene.refresh(&doc, 0).unwrap();
            assert_eq!(
                scene.block_image(1).unwrap().as_bytes().as_ptr(),
                text.as_bytes().as_ptr()
            );
            assert_eq!(
                scene.contour.as_ref().unwrap().as_bytes().as_ptr(),
                shape.as_bytes().as_ptr()
            );
        }
        if let Content::Text { text, .. } = &mut doc.labels[0].blocks[0].content {
            *text = "Changed".into();
        }
        scene.refresh(&doc, 0).unwrap();
        assert_ne!(
            scene.block_image(1).unwrap().as_bytes().as_ptr(),
            text.as_bytes().as_ptr()
        );
        assert_eq!(
            scene.contour.as_ref().unwrap().as_bytes().as_ptr(),
            shape.as_bytes().as_ptr()
        );
    }

    #[test]
    fn layered_canvas_matches_print_preview_for_round_catalog_and_wide_stock() {
        for size in [
            D101LabelSize::W12L40,
            D101LabelSize::W20L20,
            D101LabelSize::W25L25,
            D101LabelSize::W12_5L109,
        ] {
            let mut spec = LabelSpec::default();
            size.apply(&mut spec);
            let mut doc = Document::new(spec.clone());
            doc.labels[0].blocks = vec![Block {
                id: 1,
                bounds: Bounds {
                    x: 0.0,
                    y: 0.0,
                    width: spec.length_mm,
                    height: spec.width_mm,
                },
                content: Content::Rectangle {
                    stroke_mm: 1.0,
                    filled: true,
                },
            }];
            let mut scene = Scene::default();
            scene.refresh(&doc, 0).unwrap();
            let layer = image::load_from_memory(scene.block_image(1).unwrap().as_bytes())
                .unwrap()
                .to_rgba8();
            let cover = image::load_from_memory(scene.contour.as_ref().unwrap().as_bytes())
                .unwrap()
                .to_rgba8();
            let expected = doc.preview(0, true).unwrap();
            for (x, y, p) in expected.enumerate_pixels() {
                let art = 255 - layer.get_pixel(x, y)[3] as u32;
                let top = cover.get_pixel(x, y);
                for channel in 0..3 {
                    let actual =
                        (top[channel] as u32 * top[3] as u32 + art * (255 - top[3] as u32)) / 255;
                    let expected = (p[channel] as u32 * p[3] as u32
                        + WORKSPACE_COLOR[channel] as u32 * (255 - p[3] as u32))
                        / 255;
                    assert_eq!(actual, expected, "{size:?} ({x}, {y}) channel {channel}");
                }
            }
        }
    }
}
