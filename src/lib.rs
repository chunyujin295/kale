pub mod glyph;
pub mod input;
pub mod render;

use render::{Format, calculate_target_rows};

/// The four renderers the CLI exposes. `Glyph` is not a block mode: it fits a
/// set of rasterized candidate glyphs instead of enumerating sub-pixel masks.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RenderMode {
    Half,
    Quadrant,
    Braille,
    Glyph,
}

impl RenderMode {
    pub fn block_mode(self) -> Option<render::Mode> {
        match self {
            RenderMode::Half => Some(render::Mode::Half),
            RenderMode::Quadrant => Some(render::Mode::Quadrant),
            RenderMode::Braille => Some(render::Mode::Braille),
            RenderMode::Glyph => None,
        }
    }
}

pub struct Options {
    pub input: String,
    pub width: usize,
    pub mode: RenderMode,
    pub format: Format,
    pub background: [u8; 3],
    pub font: String,
    /// Leave cells that are fully transparent in the source unpainted, so the
    /// terminal's own background shows through instead of `background`.
    pub transparent: bool,
}

/// Decodes `options.input`, scales it to the render grid, and renders it.
pub fn render_image(options: &Options) -> Result<String, String> {
    let image = input::decode(&options.input)?;
    let (source_width, source_height) = (image.width(), image.height());
    if source_width == 0 || source_height == 0 {
        return Err("Could not determine image dimensions.".to_string());
    }
    let rows = calculate_target_rows(source_width, source_height, options.width)
        .ok_or_else(|| "Could not determine image dimensions.".to_string())?;

    let glyph_masks = match options.mode {
        RenderMode::Glyph => Some(glyph::build_glyph_masks(&options.font)?),
        _ => None,
    };

    // The sample grid is what one terminal cell consumes: 1x2 for half blocks,
    // 2x2 for quadrants, 2x4 for braille, and the glyph mask's own size.
    let (sample_width, sample_height) = match (&glyph_masks, options.mode.block_mode()) {
        (Some(masks), _) => {
            let first = masks.first().ok_or("At least one glyph mask is required.")?;
            (first.width, first.height)
        }
        (None, Some(mode)) => {
            let spec = render::mode_spec(mode);
            (spec.sample_width, spec.sample_height)
        }
        (None, None) => return Err("Unsupported render mode.".to_string()),
    };

    let sampled = input::flatten_and_resize(
        &image,
        (options.width * sample_width) as u32,
        (rows * sample_height) as u32,
        options.background,
    )?;

    let alpha = options.transparent.then(|| sampled.alpha.as_slice());

    match glyph_masks {
        Some(masks) => render::render_glyph_fit(
            &sampled.pixels,
            sampled.width,
            sampled.height,
            &masks,
            options.format,
            alpha,
        ),
        None => render::render_block_mode(
            &sampled.pixels,
            sampled.width,
            sampled.height,
            options
                .mode
                .block_mode()
                .ok_or("Unsupported render mode.".to_string())?,
            options.format,
            alpha,
        ),
    }
}
