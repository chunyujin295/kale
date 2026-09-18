//! Candidate glyph masks for `--mode glyph`.
//!
//! The original implementation rasterized an SVG `<text>` element through
//! librsvg, using `font-size = height`, `textLength = width` with
//! `lengthAdjust="spacingAndGlyphs"`, and a baseline at `height - 2`. This does
//! the same thing with fontdb (font lookup) and fontdue (rasterization): the
//! glyph is rasterized at `height` pixels and then squeezed horizontally so its
//! advance width fills the cell.

use fontdb::{Database, Family, Query};
use fontdue::{Font, FontSettings};

use crate::render::GlyphMask;

pub const DEFAULT_GLYPHS: &str = " .,:;i1tfLCG08@";
const CELL_WIDTH: usize = 8;
const CELL_HEIGHT: usize = 16;

pub fn build_glyph_masks(font_family: &str) -> Result<Vec<GlyphMask>, String> {
    build_glyph_masks_with(DEFAULT_GLYPHS, font_family, CELL_WIDTH, CELL_HEIGHT)
}

pub fn build_glyph_masks_with(
    glyphs: &str,
    font_family: &str,
    cell_width: usize,
    cell_height: usize,
) -> Result<Vec<GlyphMask>, String> {
    let font = load_font(font_family)?;
    // Baseline offset from the top of the cell, matching the original SVG
    // layout's `y = height - 2`.
    let baseline = (cell_height as i64 - 2) as f32;
    let mut masks = Vec::with_capacity(glyphs.chars().count());

    for glyph in glyphs.chars() {
        let (metrics, bitmap) = font.rasterize(glyph, cell_height as f32);
        let mut alpha = vec![0f64; cell_width * cell_height];

        if metrics.width > 0 && metrics.height > 0 && metrics.advance_width > 0.0 {
            // `textLength` squeezed the glyph so its advance exactly filled the cell.
            let squeeze = cell_width as f32 / metrics.advance_width;
            // Bitmap row 0 is the top of the glyph's bounding box, which sits
            // `ymin + height` above the baseline.
            let glyph_top = baseline - (metrics.ymin as f32 + metrics.height as f32);

            for y in 0..cell_height {
                for x in 0..cell_width {
                    // Sample at pixel centers, mapping cell space into the
                    // glyph's unscaled raster coordinates.
                    let source_x = (x as f32 + 0.5) / squeeze - metrics.xmin as f32;
                    let source_y = y as f32 - glyph_top;
                    alpha[y * cell_width + x] = sample_coverage(
                        &bitmap,
                        metrics.width,
                        metrics.height,
                        source_x,
                        source_y,
                    );
                }
            }
        }

        masks.push(GlyphMask {
            glyph,
            width: cell_width,
            height: cell_height,
            alpha,
        });
    }

    Ok(masks)
}

/// Bilinear sample of the coverage bitmap, normalized to 0..1. Anything outside
/// the bitmap counts as uncovered.
fn sample_coverage(bitmap: &[u8], width: usize, height: usize, x: f32, y: f32) -> f64 {
    let x0 = x.floor();
    let y0 = y.floor();
    let fx = (x - x0) as f64;
    let fy = (y - y0) as f64;
    let (ix0, iy0) = (x0 as i64, y0 as i64);

    let at = |ix: i64, iy: i64| -> f64 {
        if ix < 0 || iy < 0 || ix as usize >= width || iy as usize >= height {
            0.0
        } else {
            bitmap[iy as usize * width + ix as usize] as f64
        }
    };

    let top = at(ix0, iy0) * (1.0 - fx) + at(ix0 + 1, iy0) * fx;
    let bottom = at(ix0, iy0 + 1) * (1.0 - fx) + at(ix0 + 1, iy0 + 1) * fx;
    (top * (1.0 - fy) + bottom * fy) / 255.0
}

/// Looks up `font_family`, falling back to the generic monospace family.
fn load_font(font_family: &str) -> Result<Font, String> {
    let mut database = Database::new();
    database.load_system_fonts();

    let query = Query {
        families: &[Family::Name(font_family), Family::Monospace],
        ..Query::default()
    };
    let id = database
        .query(&query)
        .ok_or_else(|| format!("No usable font found for '{font_family}'."))?;
    let data = database
        .with_face_data(id, |data, _index| data.to_vec())
        .ok_or_else(|| format!("Could not read font data for '{font_family}'."))?;

    Font::from_bytes(data, FontSettings::default())
        .map_err(|error| format!("Could not parse font '{font_family}': {error}"))
}
