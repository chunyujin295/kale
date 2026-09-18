const ESC = '\u001b[';
export function sgrForeground([r, g, b]) { return `${ESC}38;2;${r};${g};${b}m`; }
export function sgrBackground([r, g, b]) { return `${ESC}48;2;${r};${g};${b}m`; }

const QUADRANTS = [' ', '▘', '▝', '▀', '▖', '▌', '▞', '▛', '▗', '▚', '▐', '▜', '▄', '▙', '▟', '█'];
const BRAILLE_BITS = [1, 8, 2, 16, 4, 32, 64, 128];
export const MODES = Object.freeze({
  half: { sampleWidth: 1, sampleHeight: 2, glyphs: ['▀'], masks: [1] },
  quadrant: { sampleWidth: 2, sampleHeight: 2, glyphs: QUADRANTS, masks: QUADRANTS.map((_, index) => index) },
  braille: { sampleWidth: 2, sampleHeight: 4, glyphs: Array.from({ length: 256 }, (_, index) => String.fromCodePoint(0x2800 + index)), masks: Array.from({ length: 256 }, (_, index) => index) }
});

function sameColor(a, b) { return a && b && a[0] === b[0] && a[1] === b[1] && a[2] === b[2]; }
function cssColor([r, g, b]) { return `rgb(${r} ${g} ${b})`; }
function clampByte(value) { return Math.max(0, Math.min(255, Math.round(value))); }
export function calculateTargetRows(sourceWidth, sourceHeight, columns, cellAspect = 0.5) {
  if (!Number.isFinite(sourceWidth) || !Number.isFinite(sourceHeight) || !Number.isInteger(columns) || columns < 1) throw new Error('Invalid source dimensions or column count.');
  return Math.max(1, Math.round(columns * cellAspect * sourceHeight / sourceWidth));
}
export function calculateTargetHeight(sourceWidth, sourceHeight, columns, cellAspect = 0.5) { return calculateTargetRows(sourceWidth, sourceHeight, columns, cellAspect) * 2; }

function pixelsForCell(pixels, sourceWidth, startX, startY, cellWidth, cellHeight) {
  const result = [];
  for (let y = 0; y < cellHeight; y += 1) for (let x = 0; x < cellWidth; x += 1) {
    const offset = ((startY + y) * sourceWidth + startX + x) * 3;
    result.push([pixels[offset], pixels[offset + 1], pixels[offset + 2]]);
  }
  return result;
}
function maskBit(mode, index) { return mode === 'braille' ? BRAILLE_BITS[index] : 1 << index; }
function meansAndError(samples, mask, mode) {
  const on = [0, 0, 0], off = [0, 0, 0]; let onCount = 0, offCount = 0;
  for (let index = 0; index < samples.length; index += 1) {
    const target = (mask & maskBit(mode, index)) ? on : off;
    for (let channel = 0; channel < 3; channel += 1) target[channel] += samples[index][channel];
    if (target === on) onCount += 1; else offCount += 1;
  }
  const overall = samples.reduce((sum, color) => sum.map((value, index) => value + color[index]), [0, 0, 0]).map((value) => value / samples.length);
  const foreground = (onCount ? on.map((value) => value / onCount) : overall).map(clampByte);
  const background = (offCount ? off.map((value) => value / offCount) : overall).map(clampByte);
  let error = 0;
  for (let index = 0; index < samples.length; index += 1) {
    const estimate = (mask & maskBit(mode, index)) ? foreground : background;
    for (let channel = 0; channel < 3; channel += 1) error += (samples[index][channel] - estimate[channel]) ** 2;
  }
  return { foreground, background, error };
}
function createWriter(format) {
  let previousForeground = null, previousBackground = null;
  return ({ glyph, foreground, background }) => {
    if (format === 'html') return `<span style="color:${cssColor(foreground)};background:${cssColor(background)}">${glyph}</span>`;
    let output = '';
    if (!sameColor(foreground, previousForeground)) { output += sgrForeground(foreground); previousForeground = foreground; }
    if (!sameColor(background, previousBackground)) { output += sgrBackground(background); previousBackground = background; }
    return `${output}${glyph}`;
  };
}
function finish(rows, format) { return format === 'html' ? `<pre style="line-height:1;font-family:monospace;white-space:pre">${rows.join('\n')}</pre>` : `${rows.join('\n')}${ESC}0m`; }

/** Render half blocks, quadrant blocks, or Braille using the lowest-RGB-error two-color partition. */
export function renderBlockMode(pixels, width, height, { mode = 'half', format = 'ansi' } = {}) {
  const spec = MODES[mode];
  if (!spec) throw new Error(`Unsupported block mode: ${mode}`);
  if (width % spec.sampleWidth || height % spec.sampleHeight) throw new Error(`${mode} source dimensions are not aligned to its sample grid.`);
  if (pixels.length !== width * height * 3) throw new Error('Expected packed RGB pixels.');
  const write = createWriter(format), rows = [];
  for (let y = 0; y < height; y += spec.sampleHeight) {
    let row = '';
    for (let x = 0; x < width; x += spec.sampleWidth) {
      const samples = pixelsForCell(pixels, width, x, y, spec.sampleWidth, spec.sampleHeight); let best = null;
      for (let index = 0; index < spec.masks.length; index += 1) {
        const fit = meansAndError(samples, spec.masks[index], mode);
        if (!best || fit.error < best.error) best = { ...fit, glyph: spec.glyphs[index] };
      }
      row += write(best);
    }
    rows.push(row);
  }
  return finish(rows, format);
}
export function renderHalfBlocks(pixels, width, height, options = {}) {
  if (!Number.isInteger(width) || !Number.isInteger(height) || width < 1 || height < 2) throw new Error('Image dimensions must be at least 1 × 2 pixels.');
  return renderBlockMode(pixels, width, height, { ...options, mode: 'half' });
}

function fitContinuousMask(samples, mask) {
  let aa = 0, ab = 0, bb = 0; const ay = [0, 0, 0], by = [0, 0, 0];
  for (let index = 0; index < samples.length; index += 1) {
    const a = mask[index], b = 1 - a, color = samples[index]; aa += a * a; ab += a * b; bb += b * b;
    for (let channel = 0; channel < 3; channel += 1) { ay[channel] += a * color[channel]; by[channel] += b * color[channel]; }
  }
  const determinant = aa * bb - ab * ab;
  if (Math.abs(determinant) < 0.00001) return null;
  const foreground = [], background = [];
  for (let channel = 0; channel < 3; channel += 1) {
    foreground.push(clampByte((ay[channel] * bb - by[channel] * ab) / determinant));
    background.push(clampByte((by[channel] * aa - ay[channel] * ab) / determinant));
  }
  let error = 0;
  for (let index = 0; index < samples.length; index += 1) for (let channel = 0; channel < 3; channel += 1) {
    const estimated = mask[index] * foreground[channel] + (1 - mask[index]) * background[channel]; error += (samples[index][channel] - estimated) ** 2;
  }
  return { foreground, background, error };
}
/** Render printable glyphs by fitting anti-aliased glyph coverage to each source cell. */
export function renderGlyphFit(pixels, width, height, glyphMasks, { format = 'ansi' } = {}) {
  if (!glyphMasks.length) throw new Error('At least one glyph mask is required.');
  const { width: cellWidth, height: cellHeight } = glyphMasks[0];
  if (width % cellWidth || height % cellHeight || pixels.length !== width * height * 3) throw new Error('Glyph source dimensions are invalid.');
  const write = createWriter(format), rows = [];
  for (let y = 0; y < height; y += cellHeight) {
    let row = '';
    for (let x = 0; x < width; x += cellWidth) {
      const samples = pixelsForCell(pixels, width, x, y, cellWidth, cellHeight); let best = null;
      for (const glyphMask of glyphMasks) {
        const fit = fitContinuousMask(samples, glyphMask.alpha);
        if (fit && (!best || fit.error < best.error)) best = { ...fit, glyph: glyphMask.glyph };
      }
      if (!best) { const average = samples.reduce((sum, color) => sum.map((value, index) => value + color[index]), [0, 0, 0]).map((value) => clampByte(value / samples.length)); best = { glyph: ' ', foreground: average, background: average }; }
      row += write(best);
    }
    rows.push(row);
  }
  return finish(rows, format);
}
