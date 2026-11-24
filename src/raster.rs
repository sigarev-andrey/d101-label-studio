//! Bounded PWG Raster (RaS2) decoder: black/sGray 1/8-bit and sRGB 8-bit.
use crate::bitmap::{Bitmap, DPI};
use crate::{i18n::tr, local_format};
use anyhow::{Result, bail, ensure};
use image::{GrayImage, Luma};

fn take<'a>(input: &mut &'a [u8], n: usize) -> Result<&'a [u8]> {
    ensure!(input.len() >= n, tr("Обрезанный PWG Raster"));
    let (out, rest) = input.split_at(n);
    *input = rest;
    Ok(out)
}
fn u32_at(data: &[u8], offset: usize) -> u32 {
    u32::from_be_bytes(data[offset..offset + 4].try_into().unwrap())
}

pub fn decode(mut data: &[u8]) -> Result<Vec<Bitmap>> {
    ensure!(
        take(&mut data, 4)? == b"RaS2",
        tr("Ожидался PWG Raster (RaS2). PDF/XPS напрямую не поддерживаются")
    );
    let mut pages = Vec::new();
    let mut total_pixels = 0u64;
    while !data.is_empty() {
        ensure!(pages.len() < 1000, tr("Слишком много страниц"));
        let h = take(&mut data, 1796)?;
        let width = u32_at(h, 372);
        let height = u32_at(h, 376);
        let bpc = u32_at(h, 384);
        let bpp = u32_at(h, 388);
        let stride = u32_at(h, 392) as usize;
        let space = u32_at(h, 400);
        let xdpi = u32_at(h, 276);
        let ydpi = u32_at(h, 280);
        ensure!(
            (1..=4096).contains(&width) && (1..=8192).contains(&height),
            tr("Недопустимый размер PWG Raster")
        );
        ensure!(
            (72..=1200).contains(&xdpi) && (72..=1200).contains(&ydpi),
            tr("Недопустимое разрешение PWG Raster")
        );
        ensure!(
            u32_at(h, 396) == 0,
            tr("Поддерживается только chunky PWG Raster")
        );
        ensure!(
            matches!(
                (bpc, bpp, space),
                (1, 1, 3 | 18) | (8, 8, 3 | 18) | (8, 24, 19)
            ),
            local_format!(
                "Неподдерживаемый цветовой формат PWG ({bpc}/{bpp}/{space})",
                "Unsupported PWG colour format ({bpc}/{bpp}/{space})"
            )
        );
        ensure!(
            stride == (width * bpp).div_ceil(8) as usize,
            tr("Неверная длина строки PWG")
        );
        let out_width = (width as f32 * DPI / xdpi as f32).round() as u32;
        let out_height = (height as f32 * DPI / ydpi as f32).round() as u32;
        ensure!(
            (1..=200).contains(&out_width) && (1..=1600).contains(&out_height),
            tr("Страница превышает 25 × 200 мм. Выберите формат этикетки в окне печати Windows")
        );
        total_pixels += u64::from(width) * u64::from(height);
        ensure!(
            total_pixels <= 32_000_000,
            tr("Задание превышает лимит 32 млн точек")
        );
        let mut img = GrayImage::new(width, height);
        let unit = bpp.div_ceil(8) as usize;
        let mut y = 0;
        while y < height {
            let repeat = take(&mut data, 1)?[0] as u32 + 1;
            ensure!(
                y + repeat <= height,
                tr("Повтор строки выходит за страницу PWG")
            );
            let mut row = Vec::with_capacity(stride);
            while row.len() < stride {
                let control = take(&mut data, 1)?[0];
                if control == 128 {
                    row.resize(stride, if space == 3 { 0 } else { 255 });
                } else if control <= 127 {
                    let count = control as usize + 1;
                    ensure!(
                        row.len() + count * unit <= stride,
                        tr("Переполнение RLE строки PWG")
                    );
                    let pixel = take(&mut data, unit)?;
                    for _ in 0..count {
                        row.extend_from_slice(pixel);
                    }
                } else {
                    let count = 257 - control as usize;
                    ensure!(
                        row.len() + count * unit <= stride,
                        tr("Переполнение literal строки PWG")
                    );
                    row.extend_from_slice(take(&mut data, count * unit)?);
                }
            }
            for x in 0..width {
                let value = match bpp {
                    1 => {
                        if row[x as usize / 8] & (0x80 >> (x % 8)) != 0 {
                            255
                        } else {
                            0
                        }
                    }
                    8 => row[x as usize],
                    24 => {
                        let i = x as usize * 3;
                        ((u32::from(row[i]) * 299
                            + u32::from(row[i + 1]) * 587
                            + u32::from(row[i + 2]) * 114)
                            / 1000) as u8
                    }
                    _ => bail!(tr("Неверная глубина цвета")),
                };
                let value = if space == 3 { 255 - value } else { value };
                for ry in y..y + repeat {
                    img.put_pixel(x, ry, Luma([value]));
                }
            }
            y += repeat;
        }
        if width != out_width || height != out_height {
            img = image::imageops::resize(
                &img,
                out_width,
                out_height,
                image::imageops::FilterType::Triangle,
            );
        }
        pages.push(Bitmap::from_gray(&crate::bitmap::crop_to_head(&img))?);
    }
    ensure!(!pages.is_empty(), tr("PWG не содержит страниц"));
    Ok(pages)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn header(width: u32, height: u32, bpp: u32, space: u32) -> Vec<u8> {
        let mut h = vec![0; 1796];
        for (offset, n) in [
            (276, 203),
            (280, 203),
            (372, width),
            (376, height),
            (384, bpp.min(8)),
            (388, bpp),
            (392, (width * bpp).div_ceil(8)),
            (400, space),
        ] {
            h[offset..offset + 4].copy_from_slice(&n.to_be_bytes());
        }
        h
    }
    #[test]
    fn wide_stock_preserves_scale_and_crops_only_outside_the_printhead() {
        let bytes = [
            b"RaS2".to_vec(),
            header(200, 1, 8, 18),
            vec![0, 127, 0, 71, 0],
        ]
        .concat();
        let page = decode(&bytes).unwrap().remove(0);
        assert_eq!((page.width, page.height), (192, 1));
        assert!(page.bits.iter().all(|b| *b == 255));
        let oversized = [b"RaS2".to_vec(), header(201, 1, 8, 18), vec![0, 128]].concat();
        assert!(decode(&oversized).is_err());
    }
    #[test]
    fn multiple_pages_and_polarity() {
        let bytes = [
            b"RaS2".to_vec(),
            header(8, 2, 1, 3),
            vec![1, 0, 0x80],
            header(8, 1, 1, 18),
            vec![0, 0, 0x7f],
        ]
        .concat();
        let pages = decode(&bytes).unwrap();
        assert_eq!(pages.len(), 2);
        assert_eq!(pages[0].bits, [0x80, 0x80]);
        assert_eq!(pages[1].bits, [0x80]);
    }
    #[test]
    fn rejects_run_and_page_overflow() {
        let bytes = [b"RaS2".to_vec(), header(8, 1, 1, 3), vec![0, 1, 0]].concat();
        assert!(decode(&bytes).is_err());
        let bytes = [b"RaS2".to_vec(), header(8, 1, 1, 3), vec![1, 0, 0]].concat();
        assert!(decode(&bytes).is_err());
    }
    #[test]
    fn white_fill_and_literal_rgb() {
        let blank = [b"RaS2".to_vec(), header(16, 1, 1, 3), vec![0, 128]].concat();
        assert_eq!(decode(&blank).unwrap()[0].bits, [0, 0]);
        let rgb = [
            b"RaS2".to_vec(),
            header(2, 1, 24, 19),
            vec![0, 255, 0, 0, 0, 255, 255, 255],
        ]
        .concat();
        assert_eq!(decode(&rgb).unwrap()[0].bits, [128]);
    }
    #[test]
    fn truncated_input_never_panics() {
        let bytes = [b"RaS2".to_vec(), header(8, 1, 1, 3), vec![0, 0, 0xff]].concat();
        for i in 0..bytes.len() {
            assert!(decode(&bytes[..i]).is_err());
        }
    }
}
