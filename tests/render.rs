//! Ported from the original `test/render.test.js`. These encode the exact
//! behavior the Node implementation had, so they double as the port's spec.

use kale::render::{
    Format, GlyphMask, Mode, calculate_target_height, calculate_target_rows, render_block_mode,
    render_glyph_fit, render_half_blocks,
};

#[test]
fn renders_upper_and_lower_pixels_as_true_color_foreground_and_background() {
    let output = render_half_blocks(&[255, 0, 0, 0, 0, 255], 1, 2, Format::Ansi, None).unwrap();
    assert_eq!(
        output,
        "\u{1b}[38;2;255;0;0m\u{1b}[48;2;0;0;255m▀\u{1b}[49m\u{1b}[0m"
    );
}

#[test]
fn does_not_repeat_unchanged_ansi_color_sequences_within_a_row() {
    let pixels = [1, 2, 3, 1, 2, 3, 4, 5, 6, 4, 5, 6];
    let output = render_half_blocks(&pixels, 2, 2, Format::Ansi, None).unwrap();
    assert_eq!(output.matches("38;2").count(), 1);
    assert_eq!(output.matches("48;2").count(), 1);
}

#[test]
fn calculates_an_even_two_samples_per_cell_height_with_terminal_aspect_correction() {
    assert_eq!(calculate_target_height(1600, 900, 80), Some(46));
}

#[test]
fn renders_an_html_version_without_terminal_escape_sequences() {
    let output = render_half_blocks(&[255, 0, 0, 0, 0, 255], 1, 2, Format::Html, None).unwrap();
    assert!(output.contains("color:rgb(255 0 0)"));
    assert!(output.contains("background:rgb(0 0 255)"));
    assert!(!output.contains('\u{1b}'));
}

#[test]
fn quadrant_mode_partitions_a_2_by_2_cell_into_independently_colored_areas() {
    let pixels = [255, 0, 0, 0, 0, 255, 255, 0, 0, 0, 0, 255];
    let output = render_block_mode(&pixels, 2, 2, Mode::Quadrant, Format::Ansi, None).unwrap();
    assert!(output.contains('▌'), "expected a left-half block in {output:?}");
    assert!(output.contains("38;2;255;0;0"));
    assert!(output.contains("48;2;0;0;255"));
}

#[test]
fn braille_mode_renders_a_two_by_four_sample_grid_as_one_cell() {
    let mut pixels = Vec::new();
    for index in 0..8 {
        let color: [u8; 3] = if index < 4 { [255, 255, 255] } else { [0, 0, 0] };
        pixels.extend_from_slice(&color);
    }
    let output = render_block_mode(&pixels, 2, 4, Mode::Braille, Format::Ansi, None).unwrap();
    assert!(
        output.chars().any(|c| ('\u{2800}'..='\u{28ff}').contains(&c)),
        "expected a braille codepoint in {output:?}"
    );
    assert_eq!(output.matches('\n').count(), 0);
}

#[test]
fn row_calculation_is_independent_of_renderer_sample_grid() {
    assert_eq!(calculate_target_rows(1600, 900, 80), Some(23));
}

#[test]
fn glyph_fitting_solves_foreground_and_background_from_a_coverage_mask() {
    let masks = [GlyphMask {
        glyph: 'X',
        width: 1,
        height: 2,
        alpha: vec![1.0, 0.0],
    }];
    let output = render_glyph_fit(&[255, 0, 0, 0, 0, 255], 1, 2, &masks, Format::Ansi, None).unwrap();
    assert_eq!(
        output,
        "\u{1b}[38;2;255;0;0m\u{1b}[48;2;0;0;255mX\u{1b}[49m\u{1b}[0m"
    );
}

#[test]
fn a_row_ends_with_the_background_reset() {
    // Terminals may extend whatever background is active at the end of a line
    // across the rest of that line, which would smear this cell's colour far
    // past the rendered width. Nothing should be in effect going into the
    // line break.
    let output = render_half_blocks(&[255, 0, 0, 0, 0, 255], 1, 2, Format::Ansi, None).unwrap();
    assert!(
        output.ends_with("\u{1b}[49m\u{1b}[0m"),
        "row should end with a background reset, got {output:?}"
    );
}

#[test]
fn a_row_of_unpainted_cells_needs_no_reset() {
    let pixels = [0, 0, 0, 0, 0, 0];
    let alpha = [0u8, 0];
    let output = render_half_blocks(&pixels, 1, 2, Format::Ansi, Some(&alpha)).unwrap();
    assert_eq!(output, "\u{1b}[39m\u{1b}[49m \u{1b}[0m");
}

#[test]
fn a_fully_transparent_cell_is_left_unpainted() {
    // Composited onto the default black, but every source pixel is transparent,
    // so the terminal's own background should show rather than a black block.
    let pixels = [0, 0, 0, 0, 0, 0];
    let alpha = [0u8, 0];
    let output = render_half_blocks(&pixels, 1, 2, Format::Ansi, Some(&alpha)).unwrap();
    assert_eq!(output, "\u{1b}[39m\u{1b}[49m \u{1b}[0m");
}

#[test]
fn a_partly_transparent_cell_is_still_painted() {
    let pixels = [10, 20, 30, 40, 50, 60];
    let alpha = [0u8, 255];
    let output = render_half_blocks(&pixels, 1, 2, Format::Ansi, Some(&alpha)).unwrap();
    // Every row now ends with a background reset, so "was this cell painted?"
    // is answered by the absence of the unpainted cell's foreground reset.
    assert!(
        !output.contains("[39m"),
        "the cell has an opaque pixel, so it must be painted"
    );
    assert!(output.contains("38;2;10;20;30"));
}

#[test]
fn transparency_is_ignored_when_no_alpha_is_supplied() {
    let pixels = [0, 0, 0, 0, 0, 0];
    let output = render_half_blocks(&pixels, 1, 2, Format::Ansi, None).unwrap();
    assert!(!output.contains("[39m"));
    assert!(output.contains("48;2;0;0;0"));
}

#[test]
fn transparent_cells_are_unpainted_in_glyph_mode_too() {
    let masks = [GlyphMask {
        glyph: 'X',
        width: 1,
        height: 2,
        alpha: vec![1.0, 0.0],
    }];
    let alpha = [0u8, 0];
    let output =
        render_glyph_fit(&[9, 9, 9, 9, 9, 9], 1, 2, &masks, Format::Ansi, Some(&alpha)).unwrap();
    assert_eq!(output, "\u{1b}[39m\u{1b}[49m \u{1b}[0m");
}

#[test]
fn html_marks_transparent_cells_as_transparent() {
    let pixels = [0, 0, 0, 0, 0, 0];
    let alpha = [0u8, 0];
    let output = render_half_blocks(&pixels, 1, 2, Format::Html, Some(&alpha)).unwrap();
    assert!(output.contains("background:transparent"), "got {output:?}");
}
