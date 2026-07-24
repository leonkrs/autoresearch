//! Mock-render fallback: draw a mock app screen from brand tokens when the real app cannot run (e.g.
//! iOS on a host without Xcode, or a design mock). Pure local raster (image + ab_glyph), zero network.
//! Not a pixel-perfect renderer — a clean, branded placeholder in the Calamus/Spocken card idiom.

use ab_glyph::{Font, FontVec, Glyph, Point, PxScale, ScaleFont};
use image::{imageops, DynamicImage, ImageFormat, Rgba, RgbaImage};
use std::io::Cursor;

/// One card on the mock screen: a title and an accent colour (its left spine).
pub struct Card {
    pub title: String,
    pub accent: [u8; 3],
}

/// Load a usable sans font: first a caller-provided path, else common macOS system fonts.
pub fn load_font(path: Option<&str>) -> Result<FontVec, String> {
    let candidates: Vec<String> = path
        .map(|p| vec![p.to_string()])
        .unwrap_or_else(|| {
            [
                "/System/Library/Fonts/Supplemental/Arial.ttf",
                "/System/Library/Fonts/Helvetica.ttc",
                "/Library/Fonts/Arial.ttf",
                "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect()
        });
    for c in &candidates {
        if let Ok(bytes) = std::fs::read(c) {
            if let Ok(f) = FontVec::try_from_vec(bytes) {
                return Ok(f);
            }
        }
    }
    Err("no usable font found (pass a .ttf path)".into())
}

fn draw_text(canvas: &mut RgbaImage, font: &FontVec, text: &str, x0: f32, y0: f32, px: f32, color: [u8; 3]) {
    let scale = PxScale::from(px);
    let scaled = font.as_scaled(scale);
    let ascent = scaled.ascent();
    let mut x = x0;
    for ch in text.chars() {
        let gid = font.glyph_id(ch);
        let glyph = Glyph { id: gid, scale, position: Point { x, y: y0 + ascent } };
        if let Some(outline) = font.outline_glyph(glyph) {
            let bb = outline.px_bounds();
            outline.draw(|gx, gy, cov| {
                let px_x = bb.min.x as i32 + gx as i32;
                let px_y = bb.min.y as i32 + gy as i32;
                if px_x >= 0 && px_y >= 0 && (px_x as u32) < canvas.width() && (px_y as u32) < canvas.height() {
                    let p = canvas.get_pixel_mut(px_x as u32, px_y as u32);
                    for i in 0..3 {
                        p[i] = (color[i] as f32 * cov + p[i] as f32 * (1.0 - cov)) as u8;
                    }
                }
            });
        }
        x += scaled.h_advance(gid);
    }
}

fn rounded(w: u32, h: u32, radius: u32, fill: [u8; 4]) -> RgbaImage {
    let mut img = RgbaImage::from_pixel(w, h, Rgba(fill));
    // Zero corners outside the radius.
    let r = radius.min(w / 2).min(h / 2) as i64;
    let corners = [(r, r), (w as i64 - 1 - r, r), (r, h as i64 - 1 - r), (w as i64 - 1 - r, h as i64 - 1 - r)];
    for (i, (cx, cy)) in corners.iter().enumerate() {
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
    img
}

/// Render a mock phone screen (390x844-ish density-neutral) with a header and the given cards.
pub fn render_mock(
    title: &str,
    cards: &[Card],
    bg: [u8; 3],
    surface: [u8; 4],
    text_color: [u8; 3],
    accent_header: [u8; 3],
    font: &FontVec,
) -> Result<Vec<u8>, String> {
    let (w, h) = (1080u32, 2400u32);
    let mut canvas = RgbaImage::from_pixel(w, h, Rgba([bg[0], bg[1], bg[2], 255]));
    let pad = 54f32;
    draw_text(&mut canvas, font, title, pad, 150.0, 64.0, accent_header);

    let mut y = 300f32;
    let card_w = w - (pad as u32) * 2;
    let card_h = 210u32;
    for c in cards {
        let card = rounded(card_w, card_h, 40, surface);
        imageops::overlay(&mut canvas, &card, pad as i64, y as i64);
        // accent spine
        let spine = RgbaImage::from_pixel(9, card_h - 32, Rgba([c.accent[0], c.accent[1], c.accent[2], 255]));
        imageops::overlay(&mut canvas, &spine, pad as i64 + 6, y as i64 + 16);
        draw_text(&mut canvas, font, &c.title, pad + 44.0, y + 76.0, 40.0, text_color);
        y += (card_h + 28) as f32;
        if y as u32 > h - card_h {
            break;
        }
    }
    let mut out = Vec::new();
    DynamicImage::ImageRgba8(canvas)
        .write_to(&mut Cursor::new(&mut out), ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounds_corners_transparent() {
        let img = rounded(100, 100, 20, [255, 255, 255, 255]);
        assert_eq!(img.get_pixel(0, 0)[3], 0, "corner transparent");
        assert_eq!(img.get_pixel(50, 50)[3], 255, "center opaque");
    }
}
