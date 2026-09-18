//! Compares the Rust renderer against reference output captured from the
//! original Node implementation (`tests/golden/`, generated with sharp 0.34).
//!
//! The two implementations cannot agree bit-for-bit: image decoding
//! (zune-jpeg/png vs libjpeg-turbo/libpng) and Lanczos3 resampling round
//! differently. So the assertions are layered:
//!
//! * grid shape — rows, cells per row: exact
//! * per-cell colors — every channel within `COLOR_TOLERANCE`
//! * per-cell glyph choice — at least `GLYPH_AGREEMENT` of cells match, since a
//!   near-tie can flip once the colors shift by a unit

use std::fs;
use std::path::PathBuf;

use kale::render::Format;
use kale::{Options, RenderMode, render_image};

/// Widest per-channel difference tolerated on any color component, measured only
/// on cells that chose the same glyph.
///
/// The two implementations agree exactly on decoding and on compositing alpha;
/// what is left is Lanczos3, where libvips and `image` quantize coefficients and
/// accumulate differently. That shows up almost entirely on the high-contrast
/// edges of the icon at the most extreme downscale (1254px wide source into 40
/// columns), which is where the observed worst case of 26 comes from.
const COLOR_TOLERANCE: u8 = 28;
/// Upper bound on the mean per-channel difference, which is what actually
/// describes overall fidelity. Observed worst case is 1.78; a per-cell ceiling
/// alone would not catch a change that shifts every color slightly.
const MEAN_TOLERANCE: f64 = 2.0;
/// Share of cells whose chosen glyph must match the reference. Observed worst
/// case is 0.9166 (braille on the photo, where a 2x4 sample grid offers the most
/// near-ties for a small color shift to flip).
const GLYPH_AGREEMENT: f64 = 0.91;

#[derive(Clone, Copy, PartialEq, Eq)]
struct Cell {
    glyph: char,
    foreground: [u8; 3],
    background: [u8; 3],
}

fn golden_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
        .join(name)
}

/// Splits ANSI output into cells, tracking the current color as it goes. The
/// writer only emits a sequence when the color changes, so state carries
/// across rows exactly as the renderer's writer does.
fn parse_ansi_cells(text: &str) -> Vec<Vec<Cell>> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut foreground = [0u8; 3];
    let mut background = [0u8; 3];
    let mut sequences = 0usize;
    let mut chars = text.chars().peekable();

    while let Some(character) = chars.next() {
        if character == '\u{1b}' {
            // Skip the CSI introducer, then consume up to the terminating 'm'.
            if chars.next() != Some('[') {
                continue;
            }
            let mut sequence = String::new();
            for next in chars.by_ref() {
                if next == 'm' {
                    break;
                }
                sequence.push(next);
            }
            let params: Vec<&str> = sequence.split(';').collect();
            sequences += 1;
            if params.len() == 5 && params[0] == "38" && params[1] == "2" {
                foreground = [
                    params[2].parse().unwrap(),
                    params[3].parse().unwrap(),
                    params[4].parse().unwrap(),
                ];
            } else if params.len() == 5 && params[0] == "48" && params[1] == "2" {
                background = [
                    params[2].parse().unwrap(),
                    params[3].parse().unwrap(),
                    params[4].parse().unwrap(),
                ];
            }
            // A bare reset (ESC[0m) just ends the final row.
            continue;
        }
        if character == '\n' {
            rows.push(std::mem::take(&mut row));
            continue;
        }
        // Defensive: a CRLF checkout would otherwise turn the carriage return
        // into a cell whose glyph never matches. .gitattributes pins the files
        // to LF, but a stale checkout should not look like a rendering bug.
        if character == '\r' {
            continue;
        }
        row.push(Cell {
            glyph: character,
            foreground,
            background,
        });
    }
    if !row.is_empty() {
        rows.push(row);
    }
    // Guard against a parser that silently recognizes nothing: without this, a
    // broken escape reader makes every cell compare equal at the default color.
    assert!(
        sequences > 0,
        "parsed no ANSI sequences from {} bytes of output",
        text.len()
    );
    rows
}

struct Outcome {
    cells: usize,
    glyph_matches: usize,
    /// Largest per-channel difference among cells that chose the same glyph.
    max_delta: u8,
    /// Total per-channel difference among those cells, for the mean check.
    delta_sum: u64,
}

/// Colors are only comparable where both sides picked the same glyph: a cell
/// whose mask flips is partitioning its samples differently, so its two fitted
/// colors are expected to move a lot. Glyph agreement is asserted separately.
fn compare(reference: &str, actual: &str, label: &str) -> Outcome {
    let expected = parse_ansi_cells(reference);
    let produced = parse_ansi_cells(actual);

    assert_eq!(
        expected.len(),
        produced.len(),
        "{label}: row count differs (reference {} vs actual {})",
        expected.len(),
        produced.len()
    );

    let mut outcome = Outcome {
        cells: 0,
        glyph_matches: 0,
        max_delta: 0,
        delta_sum: 0,
    };

    for (row_index, (expected_row, produced_row)) in expected.iter().zip(&produced).enumerate() {
        assert_eq!(
            expected_row.len(),
            produced_row.len(),
            "{label}: cell count differs in row {row_index} (reference {} vs actual {})",
            expected_row.len(),
            produced_row.len()
        );
        for (column, (want, got)) in expected_row.iter().zip(produced_row).enumerate() {
            outcome.cells += 1;
            if want.glyph != got.glyph {
                continue;
            }
            outcome.glyph_matches += 1;
            for channel in 0..3 {
                let foreground_delta = want.foreground[channel].abs_diff(got.foreground[channel]);
                let background_delta = want.background[channel].abs_diff(got.background[channel]);
                let delta = foreground_delta.max(background_delta);
                outcome.max_delta = outcome.max_delta.max(delta);
                outcome.delta_sum += foreground_delta as u64 + background_delta as u64;
                assert!(
                    delta <= COLOR_TOLERANCE,
                    "{label}: cell ({row_index},{column}) channel {channel} differs by {delta} \
                     (reference {:?}/{:?} vs actual {:?}/{:?})",
                    want.foreground,
                    want.background,
                    got.foreground,
                    got.background
                );
            }
        }
    }

    let agreement = outcome.glyph_matches as f64 / outcome.cells as f64;
    assert!(
        agreement >= GLYPH_AGREEMENT,
        "{label}: glyph agreement {:.4} for {} cells, {} matched (max color delta {})",
        agreement,
        outcome.cells,
        outcome.glyph_matches,
        outcome.max_delta
    );

    let mean = outcome.delta_sum as f64 / (outcome.glyph_matches * 6).max(1) as f64;
    assert!(
        mean <= MEAN_TOLERANCE,
        "{label}: mean per-channel delta {mean:.3} over {} matched cells (max was {})",
        outcome.glyph_matches,
        outcome.max_delta
    );

    outcome
}

fn options(image: &str, mode: RenderMode, width: usize) -> Options {
    Options {
        input: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(image)
            .to_string_lossy()
            .into_owned(),
        width,
        mode,
        format: Format::Ansi,
        background: [0, 0, 0],
        font: "monospace".to_string(),
        transparent: false,
    }
}

fn check(image: &str, mode: RenderMode, width: usize, name: &str) -> Outcome {
    let reference = fs::read_to_string(golden_path(name)).expect("golden file is readable");
    let actual = render_image(&options(image, mode, width)).expect("render succeeds");
    let outcome = compare(&reference, &actual, name);
    let samples = (outcome.glyph_matches * 6).max(1) as f64;
    println!(
        "{name}: {} cells, max color delta {}, mean {:.3}, glyph agreement {:.4}",
        outcome.cells,
        outcome.max_delta,
        outcome.delta_sum as f64 / samples,
        outcome.glyph_matches as f64 / outcome.cells as f64
    );
    outcome
}

#[test]
fn half_mode_matches_reference_on_a_photo() {
    check("photo.png", RenderMode::Half, 40, "photo.half.40.txt");
    check("photo.png", RenderMode::Half, 80, "photo.half.80.txt");
}

#[test]
fn quadrant_mode_matches_reference_on_a_photo() {
    check("photo.png", RenderMode::Quadrant, 40, "photo.quadrant.40.txt");
    check("photo.png", RenderMode::Quadrant, 80, "photo.quadrant.80.txt");
}

#[test]
fn braille_mode_matches_reference_on_a_photo() {
    check("photo.png", RenderMode::Braille, 40, "photo.braille.40.txt");
    check("photo.png", RenderMode::Braille, 80, "photo.braille.80.txt");
}

#[test]
fn block_modes_match_reference_on_an_icon() {
    check("img/icons/icon1.png", RenderMode::Half, 40, "icons-icon1.half.40.txt");
    check("img/icons/icon1.png", RenderMode::Half, 80, "icons-icon1.half.80.txt");
    check(
        "img/icons/icon1.png",
        RenderMode::Quadrant,
        40,
        "icons-icon1.quadrant.40.txt",
    );
    check(
        "img/icons/icon1.png",
        RenderMode::Quadrant,
        80,
        "icons-icon1.quadrant.80.txt",
    );
    check(
        "img/icons/icon1.png",
        RenderMode::Braille,
        40,
        "icons-icon1.braille.40.txt",
    );
    check(
        "img/icons/icon1.png",
        RenderMode::Braille,
        80,
        "icons-icon1.braille.80.txt",
    );
}

#[test]
fn jpeg_input_decodes_and_matches_reference() {
    check(
        "tests/fixtures/sample.jpg",
        RenderMode::Quadrant,
        4,
        "jpeg.quadrant.txt",
    );
}

#[test]
fn transparent_input_flattens_onto_the_requested_background() {
    let mut settings = options("tests/fixtures/sample-alpha.png", RenderMode::Quadrant, 4);
    settings.background = [0x1e, 0x29, 0x3b];
    let reference = fs::read_to_string(golden_path("alpha.quadrant.bg.txt")).unwrap();
    let actual = render_image(&settings).expect("render succeeds");
    compare(&reference, &actual, "alpha.quadrant.bg.txt");
}

/// The HTML path has no ANSI state to track, so check its structure and glyphs.
#[test]
fn html_output_matches_reference_structure() {
    let mut settings = options("img/icons/icon1.png", RenderMode::Quadrant, 40);
    settings.format = Format::Html;
    let reference = fs::read_to_string(golden_path("icon1.quadrant.40.html")).unwrap();
    let actual = render_image(&settings).expect("render succeeds");

    assert!(actual.starts_with("<pre style="), "expected a <pre> fragment");
    assert!(!actual.contains('\u{1b}'), "HTML output must not contain escapes");
    assert_eq!(
        reference.matches("<span").count(),
        actual.matches("<span").count(),
        "span count differs"
    );

    let glyphs = |text: &str| -> Vec<char> {
        let mut out = Vec::new();
        let mut rest = text;
        while let Some(start) = rest.find("</span>") {
            let before = &rest[..start];
            if let Some(gt) = before.rfind('>') {
                out.extend(before[gt + 1..].chars());
            }
            rest = &rest[start + "</span>".len()..];
        }
        out
    };

    // Same tolerance as the ANSI path: a cell whose mask flips is not a defect,
    // so require the agreement rate rather than an identical sequence.
    let expected = glyphs(&reference);
    let produced = glyphs(&actual);
    assert_eq!(
        expected.len(),
        produced.len(),
        "glyph count differs ({} vs {})",
        expected.len(),
        produced.len()
    );
    let matched = expected
        .iter()
        .zip(&produced)
        .filter(|(want, got)| want == got)
        .count();
    let agreement = matched as f64 / expected.len() as f64;
    assert!(
        agreement >= GLYPH_AGREEMENT,
        "HTML glyph agreement {agreement:.4} over {} cells",
        expected.len()
    );
}
