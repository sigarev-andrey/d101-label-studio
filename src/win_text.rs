//! GDI rendering uses installed Windows font families and the same 203-dpi raster for preview/print.
use crate::i18n::tr;
use crate::{bitmap::DPI, document::TextStyle};
use anyhow::{Result, ensure};
use image::{GrayImage, Luma};
use std::{mem::size_of, ptr::null_mut};
use windows_sys::Win32::{Foundation::*, Graphics::Gdi::*};

pub fn render(text: &str, style: &TextStyle, width: u32, height: u32) -> Result<GrayImage> {
    ensure!(
        width > 0 && height > 0 && u64::from(width) * u64::from(height) <= 2_000_000,
        tr("Недопустимый размер текстового блока")
    );
    if text.is_empty() {
        return Ok(GrayImage::from_pixel(width, height, Luma([255])));
    }
    // Render beyond the viewport so font ascenders/descenders can be measured before clipping.
    let scratch_height = (height + (style.points * DPI / 72.0).ceil() as u32 * 2)
        .min(2_000_000 / width)
        .max(height);
    // All handles are owned by this call; selected objects are restored before deletion.
    unsafe {
        let dc = CreateCompatibleDC(null_mut());
        ensure!(
            !dc.is_null(),
            tr("Не удалось создать контекст шрифта Windows")
        );
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width as i32,
                biHeight: -(scratch_height as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut pixels = null_mut();
        let bitmap = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut pixels, null_mut(), 0);
        if bitmap.is_null() {
            DeleteDC(dc);
            anyhow::bail!(tr("Не удалось создать изображение текста"));
        }
        let old_bitmap = SelectObject(dc, bitmap);
        let mut font = LOGFONTW {
            lfHeight: -(style.points * DPI / 72.0).round() as i32,
            lfWeight: if style.bold { 700 } else { 400 },
            lfItalic: style.italic as u8,
            lfUnderline: style.underline as u8,
            lfStrikeOut: style.strikeout as u8,
            lfCharSet: DEFAULT_CHARSET,
            lfQuality: ANTIALIASED_QUALITY,
            ..Default::default()
        };
        for (a, b) in font
            .lfFaceName
            .iter_mut()
            .take(31)
            .zip(style.family.encode_utf16())
        {
            *a = b;
        }
        let font = CreateFontIndirectW(&font);
        if font.is_null() {
            SelectObject(dc, old_bitmap);
            DeleteObject(bitmap);
            DeleteDC(dc);
            anyhow::bail!(tr("Не удалось загрузить шрифт Windows"));
        }
        let old_font = SelectObject(dc, font);
        let length = (width * scratch_height * 4) as usize;
        std::ptr::write_bytes(pixels, 255, length);
        SetBkMode(dc, TRANSPARENT as i32);
        SetTextColor(dc, 0);
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: width as i32,
            bottom: scratch_height as i32,
        };
        let utf16: Vec<u16> = text.encode_utf16().collect();
        let alignment = match style.alignment {
            1 => DT_CENTER,
            2 => DT_RIGHT,
            _ => DT_LEFT,
        };
        let flags = DT_TOP | DT_WORDBREAK | DT_NOPREFIX | alignment;
        DrawTextW(dc, utf16.as_ptr(), utf16.len() as i32, &mut rect, flags);
        GdiFlush();
        let bytes = std::slice::from_raw_parts(pixels.cast::<u8>(), length);
        let raster = GrayImage::from_fn(width, scratch_height, |x, y| {
            let i = ((y * width + x) * 4) as usize;
            Luma([if bytes[i] < 128 { 0 } else { 255 }])
        });
        SelectObject(dc, old_font);
        SelectObject(dc, old_bitmap);
        DeleteObject(font);
        DeleteObject(bitmap);
        DeleteDC(dc);
        let mut image = GrayImage::from_pixel(width, height, Luma([255]));
        let occupied = |y| (0..width).any(|x| raster.get_pixel(x, y)[0] == 0);
        if let (Some(top), Some(bottom)) = (
            (0..scratch_height).find(|&y| occupied(y)),
            (0..scratch_height).rfind(|&y| occupied(y)),
        ) {
            let ink_height = bottom - top + 1;
            let free = height.saturating_sub(ink_height);
            let offset = match style.vertical_alignment {
                1 => free / 2,
                2 => free,
                _ => 0,
            };
            for y in 0..ink_height.min(height) {
                for x in 0..width {
                    image.put_pixel(x, y + offset, *raster.get_pixel(x, y + top));
                }
            }
        }
        Ok(image)
    }
}

pub fn font_families() -> Vec<String> {
    unsafe extern "system" fn collect(
        font: *const LOGFONTW,
        _: *const TEXTMETRICW,
        _: u32,
        data: LPARAM,
    ) -> i32 {
        // EnumFontFamiliesExW invokes the callback synchronously with our live Vec.
        unsafe {
            let fonts = &mut *(data as *mut Vec<String>);
            let name = &(*font).lfFaceName;
            let n = name.iter().position(|x| *x == 0).unwrap_or(name.len());
            let name = String::from_utf16_lossy(&name[..n]);
            if !name.starts_with('@') {
                fonts.push(name);
            }
        }
        1
    }
    let mut fonts = Vec::new();
    unsafe {
        let dc = CreateCompatibleDC(null_mut());
        if !dc.is_null() {
            let font = LOGFONTW {
                lfCharSet: DEFAULT_CHARSET,
                ..Default::default()
            };
            EnumFontFamiliesExW(dc, &font, Some(collect), &mut fonts as *mut _ as isize, 0);
            DeleteDC(dc);
        }
    }
    fonts.sort();
    fonts.dedup();
    if fonts.is_empty() {
        fonts.push("Segoe UI".into());
    }
    fonts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ink(image: &GrayImage) -> Vec<(u32, u32)> {
        image
            .enumerate_pixels()
            .filter_map(|(x, y, p)| (p[0] == 0).then_some((x, y)))
            .collect()
    }

    #[test]
    fn font_ink_touches_top_or_bottom_without_changing_size_on_resize() {
        for text in ["D101", "gypйЁ", "AB\r\nCD"] {
            for underline in [false, true] {
                let style = TextStyle {
                    underline,
                    ..TextStyle::default()
                };
                let small = ink(&render(text, &style, 200, 160).unwrap());
                let large = ink(&render(text, &style, 320, 200).unwrap());
                assert!(!small.is_empty());
                assert_eq!(small.first().unwrap().1, 0);
                assert_eq!(small, large, "Resizing must not scale font ink");
                let ink_height = small.last().unwrap().1 + 1;
                let tight = ink(&render(text, &style, 200, ink_height).unwrap());
                assert_eq!(
                    tight, small,
                    "Font metrics must not clip descenders/underlines"
                );
                let bottom = ink(&render(
                    text,
                    &TextStyle {
                        vertical_alignment: 2,
                        ..style
                    },
                    200,
                    160,
                )
                .unwrap());
                assert_eq!(bottom.last().unwrap().1, 159);
                assert_eq!(
                    bottom,
                    small
                        .iter()
                        .map(|&(x, y)| (x, y + 160 - ink_height))
                        .collect::<Vec<_>>()
                );
            }
        }
    }

    #[test]
    fn vertical_alignment_moves_the_whole_wrapped_paragraph() {
        for text in ["AB\r\nCD", "AB CD EF GH IJ KL"] {
            for alignment in 0..=2 {
                let style = TextStyle {
                    alignment,
                    ..TextStyle::default()
                };
                let top = ink(&render(text, &style, 100, 200).unwrap());
                let center = ink(&render(
                    text,
                    &TextStyle {
                        vertical_alignment: 1,
                        ..style.clone()
                    },
                    100,
                    200,
                )
                .unwrap());
                let bottom = ink(&render(
                    text,
                    &TextStyle {
                        vertical_alignment: 2,
                        ..style
                    },
                    100,
                    200,
                )
                .unwrap());
                assert!(!top.is_empty());
                let center_shift = center[0].1 - top[0].1;
                let bottom_shift = bottom[0].1 - top[0].1;
                assert!(center_shift > 0 && bottom_shift > center_shift);
                assert!(bottom_shift == center_shift * 2 || bottom_shift == center_shift * 2 + 1);
                assert_eq!(
                    center,
                    top.iter()
                        .map(|&(x, y)| (x, y + center_shift))
                        .collect::<Vec<_>>()
                );
                assert_eq!(
                    bottom,
                    top.iter()
                        .map(|&(x, y)| (x, y + bottom_shift))
                        .collect::<Vec<_>>()
                );
                assert!(bottom.last().unwrap().1 > 170);
            }
        }
    }

    #[test]
    fn horizontal_alignment_combines_with_each_vertical_position() {
        for vertical_alignment in 0..=2 {
            let images: Vec<_> = (0..=2)
                .map(|alignment| {
                    ink(&render(
                        "D101",
                        &TextStyle {
                            alignment,
                            vertical_alignment,
                            ..TextStyle::default()
                        },
                        220,
                        150,
                    )
                    .unwrap())
                })
                .collect();
            let left = &images[0];
            assert!(images[1][0].0 > left[0].0 && images[2][0].0 > images[1][0].0);
            for aligned in &images[1..] {
                let shift = aligned[0].0 - left[0].0;
                assert_eq!(
                    *aligned,
                    left.iter()
                        .map(|&(x, y)| (x + shift, y))
                        .collect::<Vec<_>>()
                );
            }
        }
    }

    #[test]
    fn oversized_and_empty_text_remain_clipped_and_stable() {
        for text in ["", "One\r\nTwo\r\nThree\r\nFour"] {
            let top = render(text, &TextStyle::default(), 70, 30).unwrap();
            for vertical_alignment in 1..=2 {
                assert_eq!(
                    top,
                    render(
                        text,
                        &TextStyle {
                            vertical_alignment,
                            ..TextStyle::default()
                        },
                        70,
                        30
                    )
                    .unwrap()
                );
            }
        }
    }
}
