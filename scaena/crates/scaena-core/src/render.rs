//! Framing + store export. Pure local image work (PNG only), zero network. Wrap a raw device
//! screenshot in a padded, rounded frame; and emit store-dimension-compliant PNGs.

use image::{imageops, DynamicImage, ImageFormat, Rgba, RgbaImage};
use std::io::Cursor;

fn encode_png(img: DynamicImage) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    img.write_to(&mut Cursor::new(&mut out), ImageFormat::Png).map_err(|e| e.to_string())?;
    Ok(out)
}

/// Zero the alpha outside a rounded rectangle so the corners become transparent.
fn round_corners(img: &mut RgbaImage, radius: u32) {
    let (w, h) = img.dimensions();
    let r = radius.min(w / 2).min(h / 2) as i64;
    if r <= 0 {
        return;
    }
    let centers = [(r, r), (w as i64 - 1 - r, r), (r, h as i64 - 1 - r), (w as i64 - 1 - r, h as i64 - 1 - r)];
    for (i, (cx, cy)) in centers.iter().enumerate() {
        // Only test pixels in each corner's r x r box.
        let (x0, y0) = match i {
            0 => (0, 0),
            1 => (w as i64 - r, 0),
            2 => (0, h as i64 - r),
            _ => (w as i64 - r, h as i64 - r),
        };
        for y in y0..y0 + r {
            for x in x0..x0 + r {
                let (dx, dy) = (x - cx, y - cy);
                if dx * dx + dy * dy > r * r {
                    img.get_pixel_mut(x as u32, y as u32)[3] = 0;
                }
            }
        }
    }
}

/// Frame a screenshot: paste it, corners rounded, centered on a padded background canvas.
pub fn frame_png(src: &[u8], pad: u32, bg: [u8; 4], radius: u32) -> Result<Vec<u8>, String> {
    let mut shot = image::load_from_memory(src).map_err(|e| e.to_string())?.to_rgba8();
    round_corners(&mut shot, radius);
    let (w, h) = shot.dimensions();
    let mut canvas = RgbaImage::from_pixel(w + pad * 2, h + pad * 2, Rgba(bg));
    imageops::overlay(&mut canvas, &shot, pad as i64, pad as i64);
    encode_png(DynamicImage::ImageRgba8(canvas))
}

/// Store presets: the longest side must fall within [min, max] pixels.
pub struct StorePreset {
    pub name: &'static str,
    pub min_side: u32,
    pub max_side: u32,
}

pub const PLAY: StorePreset = StorePreset { name: "play", min_side: 320, max_side: 3840 };
pub const APPSTORE: StorePreset = StorePreset { name: "appstore", min_side: 640, max_side: 3840 };

pub fn preset(name: &str) -> Option<StorePreset> {
    match name {
        "play" => Some(PLAY),
        "appstore" => Some(APPSTORE),
        _ => None,
    }
}

/// Make a screenshot compliant with a store preset: downscale if the longest side exceeds `max_side`
/// (upscaling below `min_side` is refused because it would degrade quality; the caller should provide a
/// bigger source). Returns the compliant PNG bytes plus its final (w, h).
pub fn export_store(src: &[u8], p: &StorePreset) -> Result<(Vec<u8>, u32, u32), String> {
    let img = image::load_from_memory(src).map_err(|e| e.to_string())?;
    let (w, h) = (img.width(), img.height());
    let longest = w.max(h);
    if longest < p.min_side {
        return Err(format!("source too small for '{}' ({longest}px < {}px min)", p.name, p.min_side));
    }
    let out = if longest > p.max_side {
        let scale = p.max_side as f32 / longest as f32;
        img.resize((w as f32 * scale) as u32, (h as f32 * scale) as u32, imageops::FilterType::Lanczos3)
    } else {
        img
    };
    let (fw, fh) = (out.width(), out.height());
    Ok((encode_png(out)?, fw, fh))
}

/// Tile several screenshots into a single contact sheet (grid montage), each scaled to `thumb_w`.
pub fn contact_sheet(images: &[Vec<u8>], cols: u32, thumb_w: u32, gap: u32, bg: [u8; 4]) -> Result<Vec<u8>, String> {
    if images.is_empty() {
        return Err("no images".into());
    }
    let cols = cols.max(1);
    let mut thumbs = Vec::new();
    for b in images {
        let img = image::load_from_memory(b).map_err(|e| e.to_string())?;
        let (w, h) = (img.width().max(1), img.height());
        let th = (thumb_w as f32 * h as f32 / w as f32).round().max(1.0) as u32;
        thumbs.push(img.resize_exact(thumb_w, th, imageops::FilterType::Triangle).to_rgba8());
    }
    let n = thumbs.len() as u32;
    let rows = n.div_ceil(cols);
    let cell_h = thumbs.iter().map(|t| t.height()).max().unwrap();
    let cw = cols * thumb_w + (cols + 1) * gap;
    let ch = rows * cell_h + (rows + 1) * gap;
    let mut canvas = RgbaImage::from_pixel(cw, ch, Rgba(bg));
    for (i, t) in thumbs.iter().enumerate() {
        let (c, r) = (i as u32 % cols, i as u32 / cols);
        let x = gap + c * (thumb_w + gap);
        let y = gap + r * (cell_h + gap);
        imageops::overlay(&mut canvas, t, x as i64, y as i64);
    }
    encode_png(DynamicImage::ImageRgba8(canvas))
}

/// Scrub the macOS orange screen-recording dot from the top strip of a host capture: any orange-ish
/// pixel in the first `strip` rows is replaced with `fill`. Device screenshots never have this dot;
/// only host (`screencapture`) captures do.
pub fn scrub_orange_dot(src: &[u8], strip: u32, fill: [u8; 3]) -> Result<Vec<u8>, String> {
    let mut img = image::load_from_memory(src).map_err(|e| e.to_string())?.to_rgba8();
    let (w, h) = img.dimensions();
    let sh = strip.min(h);
    for y in 0..sh {
        for x in 0..w {
            let p = img.get_pixel(x, y);
            // Orange-ish (#FF9500 and neighbours): high R, mid G, low B.
            if p[0] > 200 && (120..200).contains(&p[1]) && p[2] < 90 {
                img.put_pixel(x, y, Rgba([fill[0], fill[1], fill[2], 255]));
            }
        }
    }
    encode_png(DynamicImage::ImageRgba8(img))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(w: u32, h: u32) -> Vec<u8> {
        let img = RgbaImage::from_pixel(w, h, Rgba([20, 100, 200, 255]));
        encode_png(DynamicImage::ImageRgba8(img)).unwrap()
    }

    #[test]
    fn framing_grows_by_padding() {
        let src = solid(200, 400);
        let framed = frame_png(&src, 24, [10, 10, 12, 255], 16).unwrap();
        let d = image::load_from_memory(&framed).unwrap();
        assert_eq!((d.width(), d.height()), (200 + 48, 400 + 48));
    }

    #[test]
    fn export_downscales_oversize() {
        let src = solid(4000, 8000); // longest 8000 > 3840
        let (bytes, w, h) = export_store(&src, &PLAY).unwrap();
        assert!(w.max(h) <= 3840, "downscaled within play max");
        assert!(image::load_from_memory(&bytes).is_ok());
    }

    #[test]
    fn export_passes_compliant() {
        let src = solid(1080, 2400);
        let (_b, w, h) = export_store(&src, &PLAY).unwrap();
        assert_eq!((w, h), (1080, 2400)); // already within spec, unchanged
    }

    #[test]
    fn contact_sheet_grid_dims() {
        let imgs = vec![solid(100, 200), solid(100, 200), solid(100, 200)];
        let sheet = contact_sheet(&imgs, 2, 100, 10, [0, 0, 0, 255]).unwrap();
        let d = image::load_from_memory(&sheet).unwrap();
        // 2 cols: width = 2*100 + 3*10 = 230; 2 rows (3 imgs): height = 2*200 + 3*10 = 430
        assert_eq!((d.width(), d.height()), (230, 430));
    }

    #[test]
    fn scrub_removes_orange_in_strip_only() {
        // Canvas with an orange pixel in the top strip and one below it.
        let mut img = RgbaImage::from_pixel(40, 200, Rgba([10, 10, 12, 255]));
        img.put_pixel(5, 5, Rgba([255, 149, 0, 255])); // orange, in strip
        img.put_pixel(5, 120, Rgba([255, 149, 0, 255])); // orange, below strip
        let src = encode_png(DynamicImage::ImageRgba8(img)).unwrap();
        let out = scrub_orange_dot(&src, 60, [28, 28, 30]).unwrap();
        let d = image::load_from_memory(&out).unwrap().to_rgba8();
        assert_eq!(d.get_pixel(5, 5).0, [28, 28, 30, 255], "orange in strip scrubbed");
        assert_eq!(d.get_pixel(5, 120).0, [255, 149, 0, 255], "orange below strip untouched");
    }
}
