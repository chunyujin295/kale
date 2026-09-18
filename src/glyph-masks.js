import sharp from 'sharp';

const DEFAULT_GLYPHS = ' .,:;i1tfLCG08@';

/** Rasterize candidate glyphs once so fitting respects their anti-aliased stroke coverage. */
export async function buildGlyphMasks({ glyphs = DEFAULT_GLYPHS, font = 'monospace', width = 8, height = 16 } = {}) {
  const escapedFont = font.replaceAll('&', '&amp;').replaceAll('"', '&quot;').replaceAll('<', '&lt;');
  const masks = [];
  for (const glyph of glyphs) {
    const escapedGlyph = glyph === ' ' ? '&#32;' : glyph.replaceAll('&', '&amp;').replaceAll('<', '&lt;');
    const svg = `<svg width="${width}" height="${height}" viewBox="0 0 ${width} ${height}" xmlns="http://www.w3.org/2000/svg"><text x="0" y="${height - 2}" fill="white" font-family="${escapedFont}" font-size="${height}" textLength="${width}" lengthAdjust="spacingAndGlyphs">${escapedGlyph}</text></svg>`;
    const { data } = await sharp(Buffer.from(svg)).ensureAlpha().raw().toBuffer({ resolveWithObject: true });
    const alpha = [];
    for (let index = 0; index < data.length; index += 4) alpha.push((data[index] * data[index + 3]) / 65025);
    masks.push({ glyph, width, height, alpha });
  }
  return masks;
}
