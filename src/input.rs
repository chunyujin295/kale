//! Image decoding, alpha flattening, and the downscale to the render grid.
//! Replaces the `sharp(...).flatten().resize(lanczos3).removeAlpha().raw()`
//! chain from the original `src/cli.js`.

use image::imageops::FilterType;
use image::{DynamicImage, ImageBuffer, Luma, Rgb, Rgba};

pub type Rgb8 = [u8; 3];

/// A decoded image ready for rendering: packed RGB plus the source alpha, both
/// already sized to the render grid (`columns * sample_width` by
/// `rows * sample_height`).
///
/// Alpha is carried through so the renderer can tell a cell that merely happens
/// to match the background color from one that is genuinely transparent.
pub struct Sampled {
    pub pixels: Vec<u8>,
    pub alpha: Vec<u8>,
    pub width: usize,
    pub height: usize,
}

/// Parses `#rrggbb`.
pub fn parse_hex_color(value: &str) -> Option<Rgb8> {
    let bytes = value.as_bytes();
    if bytes.len() != 7 || bytes[0] != b'#' {
        return None;
    }
    let digit = |byte: u8| (byte as char).to_digit(16);
    let mut channels = [0u8; 3];
    for index in 0..3 {
        let high = digit(bytes[1 + index * 2])?;
        let low = digit(bytes[2 + index * 2])?;
        channels[index] = (high * 16 + low) as u8;
    }
    Some(channels)
}

/// Opens an image, reporting failures the way sharp did so the CLI's messages
/// stay the same as the Node implementation's.
pub fn decode(path: &str) -> Result<DynamicImage, String> {
    if !std::path::Path::new(path).exists() {
        return Err(format!("Input file is missing: {path}"));
    }
    image::open(path).map_err(|_| "Input file contains unsupported image format".to_string())
}

/// Composites any alpha onto `background`, then resizes to exactly
/// `target_width` x `target_height`.
///
/// Flattening happens before the resize, matching the order the original
/// pipeline requested it in.
pub fn flatten_and_resize(
    image: &DynamicImage,
    target_width: u32,
    target_height: u32,
    background: Rgb8,
) -> Result<Sampled, String> {
    if target_width == 0 || target_height == 0 {
        return Err("Target dimensions must be non-zero.".to_string());
    }

    let rgba = image.to_rgba8();
    let (width, height) = rgba.dimensions();

    let mut flattened: ImageBuffer<Rgb<u8>, Vec<u8>> = ImageBuffer::new(width, height);
    let mut source_alpha: ImageBuffer<Luma<u8>, Vec<u8>> = ImageBuffer::new(width, height);
    for (x, y, pixel) in rgba.enumerate_pixels() {
        let Rgba([r, g, b, a]) = *pixel;
        let alpha = a as u32;
        let source = [r, g, b];
        let mut out = [0u8; 3];
        for channel in 0..3 {
            // Integer src-over with truncating division, which is what libvips
            // does for uchar images. Rounding here instead would shift roughly a
            // fifth of all output pixels by one.
            let value =
                (source[channel] as u32 * alpha + background[channel] as u32 * (255 - alpha)) / 255;
            out[channel] = value as u8;
        }
        flattened.put_pixel(x, y, Rgb(out));
        source_alpha.put_pixel(x, y, Luma([a]));
    }

    // sharp leaves the image untouched when the target already matches, and its
    // identity path is exact; running the convolution anyway would shift pixels
    // that should not move.
    if flattened.width() == target_width && flattened.height() == target_height {
        return Ok(Sampled {
            width: target_width as usize,
            height: target_height as usize,
            pixels: flattened.into_raw(),
            alpha: source_alpha.into_raw(),
        });
    }

    let resized = image::imageops::resize(
        &flattened,
        target_width,
        target_height,
        FilterType::Lanczos3,
    );
    // Triangle rather than Lanczos: only "is this cell entirely transparent" is
    // read off this plane, and Lanczos overshoot could drive a mostly opaque
    // cell to zero. A filter with no negative lobes keeps transparent regions
    // zero and lets opaque neighbours bleed in, which errs toward painting.
    let resized_alpha = image::imageops::resize(
        &source_alpha,
        target_width,
        target_height,
        FilterType::Triangle,
    );

    Ok(Sampled {
        width: resized.width() as usize,
        height: resized.height() as usize,
        pixels: resized.into_raw(),
        alpha: resized_alpha.into_raw(),
    })
}
