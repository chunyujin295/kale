//! Candidate glyph masks for `--mode glyph`.
//!
//! The original implementation rasterized an SVG `<text>` element through
//! librsvg, using `font-size = height`, `textLength = width` with
//! `lengthAdjust="spacingAndGlyphs"`, and a baseline at `height - 2`. This does
//! the same thing with fontdb (font lookup) and fontdue (rasterization): the
//! glyph is rasterized at `height` pixels and then squeezed horizontally so its
//! advance width fills the cell.

use std::path::PathBuf;

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

/// Well-known font directories, used when the environment-driven scan comes up
/// empty.
///
/// `Database::load_system_fonts()` builds its search paths out of `SYSTEMROOT`
/// and `USERPROFILE`. Shells derived from MSYS2 — Git Bash, and terminals that
/// inherit their environment — rewrite those to POSIX form, so the scan ends up
/// looking in something like `/c/Windows\Fonts` and finds nothing. Paths here
/// are hard-coded rather than read from the environment, which is the entire
/// point: they cannot be rewritten out from under us.
fn fallback_font_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();

    #[cfg(target_os = "windows")]
    {
        dirs.push(PathBuf::from(r"C:\Windows\Fonts"));
        for variable in ["LOCALAPPDATA", "USERPROFILE"] {
            // Only usable when the shell left them in Windows form.
            if let Ok(value) = std::env::var(variable)
                && value.starts_with(|c: char| c.is_ascii_alphabetic())
            {
                dirs.push(PathBuf::from(value).join(r"AppData\Local\Microsoft\Windows\Fonts"));
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        dirs.push(PathBuf::from("/System/Library/Fonts"));
        dirs.push(PathBuf::from("/Library/Fonts"));
        if let Ok(home) = std::env::var("HOME") {
            dirs.push(PathBuf::from(home).join("Library/Fonts"));
        }
    }

    #[cfg(target_os = "linux")]
    {
        dirs.push(PathBuf::from("/usr/share/fonts"));
        dirs.push(PathBuf::from("/usr/local/share/fonts"));
        if let Ok(home) = std::env::var("HOME") {
            dirs.push(PathBuf::from(&home).join(".fonts"));
            dirs.push(PathBuf::from(&home).join(".local/share/fonts"));
        }
    }

    dirs
}

fn query_font(database: &Database, font_family: &str) -> Option<fontdb::ID> {
    database.query(&Query {
        families: &[Family::Name(font_family), Family::Monospace],
        ..Query::default()
    })
}

/// Looks up `font_family`, falling back to the generic monospace family.
fn load_font(font_family: &str) -> Result<Font, String> {
    let mut database = Database::new();
    database.load_system_fonts();

    let mut found = query_font(&database, font_family);
    if found.is_none() {
        for directory in fallback_font_dirs() {
            if directory.is_dir() {
                database.load_fonts_dir(directory);
            }
        }
        found = query_font(&database, font_family);
    }

    let id = found.ok_or_else(|| {
        format!(
            "No usable font found for '{font_family}' ({} fonts available).",
            database.len()
        )
    })?;
    let data = database
        .with_face_data(id, |data, _index| data.to_vec())
        .ok_or_else(|| format!("Could not read font data for '{font_family}'."))?;

    Font::from_bytes(data, FontSettings::default())
        .map_err(|error| format!("Could not parse font '{font_family}': {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Guards the fallback that rescues shells rewriting `SYSTEMROOT` into a
    /// POSIX path (Git Bash and anything inheriting its environment). With the
    /// environment-driven scan finding nothing, these directories are the only
    /// source of fonts, so if they stop working glyph mode breaks for those
    /// users entirely.
    #[test]
    fn the_fallback_directory_scan_finds_fonts() {
        let mut database = Database::new();
        let directories = fallback_font_dirs();
        for directory in &directories {
            database.load_fonts_dir(directory);
        }
        assert!(
            !database.is_empty(),
            "no fonts found in {directories:?}"
        );
    }

    #[test]
    fn the_default_family_resolves_to_a_usable_font() {
        let masks = build_glyph_masks("monospace").expect("monospace should resolve");
        assert_eq!(masks.len(), DEFAULT_GLYPHS.chars().count());
    }
}
