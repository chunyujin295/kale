//! Color fitting and cell rendering. Ported line-for-line from the original
//! `src/render.js`; the arithmetic is intentionally kept in the same order so
//! results stay comparable with the reference outputs in `tests/golden/`.

const ESC: &str = "\u{1b}[";

pub type Rgb = [u8; 3];

const QUADRANTS: [char; 16] = [
    ' ', '▘', '▝', '▀', '▖', '▌', '▞', '▛', '▗', '▚', '▐', '▜', '▄', '▙', '▟', '█',
];

/// Bit value of the dot at each of the eight sample positions, in the 2x4 grid
/// order the Braille block lays them out. Not ascending: the grid is
/// left column top-to-bottom, then right column top-to-bottom.
const BRAILLE_BITS: [u16; 8] = [1, 8, 2, 16, 4, 32, 64, 128];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Half,
    Quadrant,
    Braille,
}

impl Mode {
    pub fn name(self) -> &'static str {
        match self {
            Mode::Half => "half",
            Mode::Quadrant => "quadrant",
            Mode::Braille => "braille",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Format {
    Ansi,
    Html,
}

pub struct ModeSpec {
    pub sample_width: usize,
    pub sample_height: usize,
    glyphs: Vec<char>,
    masks: Vec<u16>,
}

pub fn mode_spec(mode: Mode) -> ModeSpec {
    match mode {
        Mode::Half => ModeSpec {
            sample_width: 1,
            sample_height: 2,
            glyphs: vec!['▀'],
            masks: vec![1],
        },
        Mode::Quadrant => ModeSpec {
            sample_width: 2,
            sample_height: 2,
            glyphs: QUADRANTS.to_vec(),
            masks: (0..16).collect(),
        },
        Mode::Braille => ModeSpec {
            sample_width: 2,
            sample_height: 4,
            glyphs: (0..256)
                .map(|index| char::from_u32(0x2800 + index).expect("valid braille codepoint"))
                .collect(),
            masks: (0..256).collect(),
        },
    }
}

fn mask_bit(mode: Mode, index: usize) -> u16 {
    match mode {
        Mode::Braille => BRAILLE_BITS[index],
        _ => 1u16 << index,
    }
}

fn clamp_byte(value: f64) -> u8 {
    value.round().clamp(0.0, 255.0) as u8
}

fn sgr_foreground([r, g, b]: Rgb) -> String {
    format!("{ESC}38;2;{r};{g};{b}m")
}

fn sgr_background([r, g, b]: Rgb) -> String {
    format!("{ESC}48;2;{r};{g};{b}m")
}

/// Escapes a glyph for the HTML output path. None of the built-in glyphs need
/// it, but it keeps the output well-formed if a custom glyph set is ever added.
fn escape_html(glyph: char) -> String {
    match glyph {
        '&' => "&amp;".to_string(),
        '<' => "&lt;".to_string(),
        '>' => "&gt;".to_string(),
        _ => glyph.to_string(),
    }
}

/// Terminal cell aspect ratio: a character cell is about twice as tall as it is
/// wide, so one column of output covers two source rows.
pub fn calculate_target_rows(
    source_width: u32,
    source_height: u32,
    columns: usize,
) -> Option<usize> {
    if source_width == 0 || source_height == 0 || columns < 1 {
        return None;
    }
    let rows = columns as f64 * 0.5 * source_height as f64 / source_width as f64;
    Some((rows.round() as usize).max(1))
}

pub fn calculate_target_height(
    source_width: u32,
    source_height: u32,
    columns: usize,
) -> Option<usize> {
    calculate_target_rows(source_width, source_height, columns).map(|rows| rows * 2)
}

fn pixels_for_cell(
    pixels: &[u8],
    source_width: usize,
    start_x: usize,
    start_y: usize,
    cell_width: usize,
    cell_height: usize,
) -> Vec<Rgb> {
    let mut result = Vec::with_capacity(cell_width * cell_height);
    for y in 0..cell_height {
        for x in 0..cell_width {
            let offset = ((start_y + y) * source_width + start_x + x) * 3;
            result.push([pixels[offset], pixels[offset + 1], pixels[offset + 2]]);
        }
    }
    result
}

struct Fit {
    foreground: Rgb,
    background: Rgb,
    error: f64,
}

/// Splits the cell's samples into covered and uncovered by `mask`, averages each
/// side into a color, and scores the total squared error of that two-color guess.
fn means_and_error(samples: &[Rgb], mask: u16, mode: Mode) -> Fit {
    let mut on = [0f64; 3];
    let mut off = [0f64; 3];
    let mut on_count = 0usize;
    let mut off_count = 0usize;
    for (index, sample) in samples.iter().enumerate() {
        let covered = mask & mask_bit(mode, index) != 0;
        let target = if covered { &mut on } else { &mut off };
        for channel in 0..3 {
            target[channel] += sample[channel] as f64;
        }
        if covered {
            on_count += 1;
        } else {
            off_count += 1;
        }
    }

    let mut overall = [0f64; 3];
    for sample in samples {
        for channel in 0..3 {
            overall[channel] += sample[channel] as f64;
        }
    }
    let count = samples.len() as f64;
    for channel in 0..3 {
        overall[channel] /= count;
    }

    let foreground = if on_count > 0 {
        on.map(|value| clamp_byte(value / on_count as f64))
    } else {
        overall.map(clamp_byte)
    };
    let background = if off_count > 0 {
        off.map(|value| clamp_byte(value / off_count as f64))
    } else {
        overall.map(clamp_byte)
    };

    let mut error = 0f64;
    for (index, sample) in samples.iter().enumerate() {
        let covered = mask & mask_bit(mode, index) != 0;
        let estimate = if covered { foreground } else { background };
        for channel in 0..3 {
            error += (sample[channel] as f64 - estimate[channel] as f64).powi(2);
        }
    }

    Fit {
        foreground,
        background,
        error,
    }
}

/// Least-squares solve for the two colors that best explain `samples` under a
/// continuous coverage mask, by minimizing sum((a*fg + (1-a)*bg) - sample)^2.
fn fit_continuous_mask(samples: &[Rgb], mask: &[f64]) -> Option<Fit> {
    let (mut aa, mut ab, mut bb) = (0f64, 0f64, 0f64);
    let mut ay = [0f64; 3];
    let mut by = [0f64; 3];
    for (index, sample) in samples.iter().enumerate() {
        let a = mask[index];
        let b = 1.0 - a;
        aa += a * a;
        ab += a * b;
        bb += b * b;
        for channel in 0..3 {
            ay[channel] += a * sample[channel] as f64;
            by[channel] += b * sample[channel] as f64;
        }
    }

    // Degenerate when every sample has the same coverage — no colors to solve for.
    let determinant = aa * bb - ab * ab;
    if determinant.abs() < 0.00001 {
        return None;
    }

    let mut foreground = [0u8; 3];
    let mut background = [0u8; 3];
    for channel in 0..3 {
        foreground[channel] = clamp_byte((ay[channel] * bb - by[channel] * ab) / determinant);
        background[channel] = clamp_byte((by[channel] * aa - ay[channel] * ab) / determinant);
    }

    let mut error = 0f64;
    for (index, sample) in samples.iter().enumerate() {
        for channel in 0..3 {
            let estimated =
                mask[index] * foreground[channel] as f64 + (1.0 - mask[index]) * background[channel] as f64;
            error += (sample[channel] as f64 - estimated).powi(2);
        }
    }

    Some(Fit {
        foreground,
        background,
        error,
    })
}

/// What a cell asks the terminal to paint with. `Default` means "the terminal's
/// own colors", which is how a fully transparent cell stays transparent.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Paint {
    Color(Rgb),
    Default,
}

/// Emits cells while suppressing ANSI color sequences that would repeat the
/// previous one. The suppression state carries across rows, matching the
/// original writer, which is constructed once per render.
struct Writer {
    format: Format,
    previous_foreground: Option<Paint>,
    previous_background: Option<Paint>,
}

impl Writer {
    fn new(format: Format) -> Self {
        Self {
            format,
            previous_foreground: None,
            previous_background: None,
        }
    }

    fn write(&mut self, out: &mut String, glyph: char, foreground: Rgb, background: Paint) {
        if self.format == Format::Html {
            let [r, g, b] = foreground;
            match background {
                Paint::Color([br, bg, bb]) => out.push_str(&format!(
                    "<span style=\"color:rgb({r} {g} {b});background:rgb({br} {bg} {bb})\">{}</span>",
                    escape_html(glyph)
                )),
                Paint::Default => out.push_str(&format!(
                    "<span style=\"color:rgb({r} {g} {b});background:transparent\">{}</span>",
                    escape_html(glyph)
                )),
            }
            return;
        }
        self.paint(out, Paint::Color(foreground), background);
        out.push(glyph);
    }

    /// A cell whose source pixels are all fully transparent. Nothing is painted,
    /// so whatever the terminal uses as its own background shows through instead
    /// of a rectangle of `--background`.
    fn write_transparent(&mut self, out: &mut String) {
        if self.format == Format::Html {
            out.push_str("<span style=\"background:transparent\"> </span>");
            return;
        }
        self.paint(out, Paint::Default, Paint::Default);
        out.push(' ');
    }

    /// Ends a row with no background in effect.
    ///
    /// Terminals are free to extend whatever background colour is active when a
    /// line ends across the remainder of that line, which smears the last
    /// cell's colour out to the right edge — far past the rendered width. Going
    /// into the line break with the background already reset leaves nothing to
    /// extend.
    fn end_row(&mut self, out: &mut String) {
        if self.format == Format::Ansi
            && matches!(self.previous_background, Some(Paint::Color(_)))
        {
            out.push_str(&format!("{ESC}49m"));
            self.previous_background = Some(Paint::Default);
        }
    }

    fn paint(&mut self, out: &mut String, foreground: Paint, background: Paint) {
        if self.previous_foreground != Some(foreground) {
            match foreground {
                Paint::Color(color) => out.push_str(&sgr_foreground(color)),
                Paint::Default => out.push_str(&format!("{ESC}39m")),
            }
            self.previous_foreground = Some(foreground);
        }
        if self.previous_background != Some(background) {
            match background {
                Paint::Color(color) => out.push_str(&sgr_background(color)),
                Paint::Default => out.push_str(&format!("{ESC}49m")),
            }
            self.previous_background = Some(background);
        }
    }
}

/// How much of a cell's source is fully transparent.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Transparency {
    /// No transparent pixels: the cell is entirely image content.
    None,
    /// Some pixels are transparent but not all. Part of this cell is nothing at
    /// all, so whatever background it is given is a guess — the terminal's own
    /// background is the honest answer.
    Partial,
    /// Nothing but transparency: the cell should not be painted at all.
    Full,
}

fn cell_transparency(
    alpha: &[u8],
    source_width: usize,
    start_x: usize,
    start_y: usize,
    cell_width: usize,
    cell_height: usize,
) -> Transparency {
    let total = cell_width * cell_height;
    let mut transparent = 0usize;
    for y in 0..cell_height {
        for x in 0..cell_width {
            if alpha[(start_y + y) * source_width + start_x + x] == 0 {
                transparent += 1;
            }
        }
    }
    if transparent == 0 {
        Transparency::None
    } else if transparent == total {
        Transparency::Full
    } else {
        Transparency::Partial
    }
}

fn finish(rows: &[String], format: Format) -> String {
    let body = rows.join("\n");
    match format {
        Format::Html => format!(
            "<pre style=\"line-height:1;font-family:monospace;white-space:pre\">{body}</pre>"
        ),
        Format::Ansi => format!("{body}{ESC}0m"),
    }
}

/// Render half blocks, quadrant blocks, or Braille using the lowest-RGB-error
/// two-color partition.
///
/// When `alpha` is supplied and a cell's source pixels are all fully
/// transparent, that cell is left unpainted rather than filled with the color
/// alpha was composited onto.
pub fn render_block_mode(
    pixels: &[u8],
    width: usize,
    height: usize,
    mode: Mode,
    format: Format,
    alpha: Option<&[u8]>,
) -> Result<String, String> {
    let spec = mode_spec(mode);
    if width % spec.sample_width != 0 || height % spec.sample_height != 0 {
        return Err(format!(
            "{} source dimensions are not aligned to its sample grid.",
            mode.name()
        ));
    }
    if pixels.len() != width * height * 3 {
        return Err("Expected packed RGB pixels.".to_string());
    }
    if let Some(alpha) = alpha
        && alpha.len() != width * height
    {
        return Err("Expected one alpha value per pixel.".to_string());
    }

    let mut writer = Writer::new(format);
    let mut rows = Vec::with_capacity(height / spec.sample_height);
    let mut y = 0;
    while y < height {
        let mut row = String::new();
        let mut x = 0;
        while x < width {
            let transparency = alpha.map_or(Transparency::None, |alpha| {
                cell_transparency(alpha, width, x, y, spec.sample_width, spec.sample_height)
            });
            if transparency == Transparency::Full {
                writer.write_transparent(&mut row);
                x += spec.sample_width;
                continue;
            }

            let samples = pixels_for_cell(
                pixels,
                width,
                x,
                y,
                spec.sample_width,
                spec.sample_height,
            );
            let mut best: Option<(usize, Fit)> = None;
            for (index, mask) in spec.masks.iter().enumerate() {
                let fit = means_and_error(&samples, *mask, mode);
                if best.as_ref().is_none_or(|(_, current)| fit.error < current.error) {
                    best = Some((index, fit));
                }
            }
            let (index, fit) = best.expect("every mode provides at least one mask");
            let background = match transparency {
                Transparency::Partial => Paint::Default,
                _ => Paint::Color(fit.background),
            };
            writer.write(&mut row, spec.glyphs[index], fit.foreground, background);
            x += spec.sample_width;
        }
        writer.end_row(&mut row);
        rows.push(row);
        y += spec.sample_height;
    }

    Ok(finish(&rows, format))
}

pub fn render_half_blocks(
    pixels: &[u8],
    width: usize,
    height: usize,
    format: Format,
    alpha: Option<&[u8]>,
) -> Result<String, String> {
    if width < 1 || height < 2 {
        return Err("Image dimensions must be at least 1 × 2 pixels.".to_string());
    }
    render_block_mode(pixels, width, height, Mode::Half, format, alpha)
}

pub struct GlyphMask {
    pub glyph: char,
    pub width: usize,
    pub height: usize,
    pub alpha: Vec<f64>,
}

/// Render printable glyphs by fitting anti-aliased glyph coverage to each cell.
pub fn render_glyph_fit(
    pixels: &[u8],
    width: usize,
    height: usize,
    glyph_masks: &[GlyphMask],
    format: Format,
    alpha: Option<&[u8]>,
) -> Result<String, String> {
    let Some(first) = glyph_masks.first() else {
        return Err("At least one glyph mask is required.".to_string());
    };
    let (cell_width, cell_height) = (first.width, first.height);
    if width % cell_width != 0 || height % cell_height != 0 || pixels.len() != width * height * 3 {
        return Err("Glyph source dimensions are invalid.".to_string());
    }
    if let Some(alpha) = alpha
        && alpha.len() != width * height
    {
        return Err("Expected one alpha value per pixel.".to_string());
    }

    let mut writer = Writer::new(format);
    let mut rows = Vec::with_capacity(height / cell_height);
    let mut y = 0;
    while y < height {
        let mut row = String::new();
        let mut x = 0;
        while x < width {
            let transparency = alpha.map_or(Transparency::None, |alpha| {
                cell_transparency(alpha, width, x, y, cell_width, cell_height)
            });
            if transparency == Transparency::Full {
                writer.write_transparent(&mut row);
                x += cell_width;
                continue;
            }

            let samples = pixels_for_cell(pixels, width, x, y, cell_width, cell_height);
            let mut best: Option<(char, Fit)> = None;
            for glyph_mask in glyph_masks {
                if let Some(fit) = fit_continuous_mask(&samples, &glyph_mask.alpha)
                    && best.as_ref().is_none_or(|(_, current)| fit.error < current.error)
                {
                    best = Some((glyph_mask.glyph, fit));
                }
            }
            let (glyph, fit) = best.unwrap_or_else(|| {
                // Degenerate cell: a blank with the cell's average color.
                let mut average = [0f64; 3];
                for sample in &samples {
                    for channel in 0..3 {
                        average[channel] += sample[channel] as f64;
                    }
                }
                let count = samples.len() as f64;
                let color = average.map(|value| clamp_byte(value / count));
                (
                    ' ',
                    Fit {
                        foreground: color,
                        background: color,
                        error: 0.0,
                    },
                )
            });
            let background = match transparency {
                Transparency::Partial => Paint::Default,
                _ => Paint::Color(fit.background),
            };
            writer.write(&mut row, glyph, fit.foreground, background);
            x += cell_width;
        }
        writer.end_row(&mut row);
        rows.push(row);
        y += cell_height;
    }

    Ok(finish(&rows, format))
}
