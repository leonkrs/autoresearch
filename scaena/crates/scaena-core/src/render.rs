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
/// (upscaling below `min_side` is refused — it would degrade quality; the caller should provide a
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
}
