#!/usr/bin/env node
import sharp from 'sharp';
import { buildGlyphMasks } from './glyph-masks.js';
import { MODES, calculateTargetRows, renderBlockMode, renderGlyphFit } from './render.js';

function usage() {
  return `kale — true-color terminal image renderer

Usage:
  kale <image> [--mode <mode>] [--width <columns>] [--format ansi|html] [--background <#rrggbb>]

Options:
  -w, --width       Terminal columns (default: 80)
  -m, --mode        half (default), quadrant, braille, or glyph
  -f, --format      ansi (default) or html
  -b, --background  Flatten transparent pixels onto this color (default: #000000)
      --font        Font family used to calibrate glyph mode (default: monospace)
  -h, --help        Show this help
`;
}

function parseArgs(args) {
  const result = { width: 80, mode: 'half', format: 'ansi', background: '#000000', font: 'monospace', input: null };
  for (let index = 0; index < args.length; index += 1) {
    const arg = args[index];
    if (arg === '-h' || arg === '--help') return { help: true };
    if (arg === '-w' || arg === '--width') result.width = Number(args[++index]);
    else if (arg === '-m' || arg === '--mode') result.mode = args[++index];
    else if (arg === '-f' || arg === '--format') result.format = args[++index];
    else if (arg === '-b' || arg === '--background') result.background = args[++index];
    else if (arg === '--font') result.font = args[++index];
    else if (arg.startsWith('-')) throw new Error(`Unknown option: ${arg}`);
    else if (result.input) throw new Error('Only one input image is supported.');
    else result.input = arg;
  }
  if (!result.input) throw new Error('An input image is required.');
  if (!Number.isInteger(result.width) || result.width < 1 || result.width > 1000) throw new Error('--width must be an integer from 1 to 1000.');
  if (!['half', 'quadrant', 'braille', 'glyph'].includes(result.mode)) throw new Error('--mode must be half, quadrant, braille, or glyph.');
  if (!['ansi', 'html'].includes(result.format)) throw new Error('--format must be ansi or html.');
  if (!/^#[0-9a-fA-F]{6}$/.test(result.background)) throw new Error('--background must be in #rrggbb form.');
  return result;
}

async function main() {
  const options = parseArgs(process.argv.slice(2));
  if (options.help) {
    process.stdout.write(usage());
    return;
  }

  const metadata = await sharp(options.input).metadata();
  if (!metadata.width || !metadata.height) throw new Error('Could not determine image dimensions.');
  const rows = calculateTargetRows(metadata.width, metadata.height, options.width);
  const blockSpec = MODES[options.mode];
  const glyphMasks = options.mode === 'glyph' ? await buildGlyphMasks({ font: options.font }) : null;
  const sampleWidth = glyphMasks ? glyphMasks[0].width : blockSpec.sampleWidth;
  const sampleHeight = glyphMasks ? glyphMasks[0].height : blockSpec.sampleHeight;
  const { data, info } = await sharp(options.input)
    .flatten({ background: options.background })
    .resize(options.width * sampleWidth, rows * sampleHeight, { fit: 'fill', kernel: sharp.kernel.lanczos3 })
    .removeAlpha()
    .raw()
    .toBuffer({ resolveWithObject: true });

  const output = glyphMasks
    ? renderGlyphFit(data, info.width, info.height, glyphMasks, { format: options.format })
    : renderBlockMode(data, info.width, info.height, { mode: options.mode, format: options.format });
  process.stdout.write(output);
  process.stdout.write('\n');
}

main().catch((error) => {
  process.stderr.write(`kale: ${error.message}\n`);
  process.exitCode = 1;
});
