import test from 'node:test';
import assert from 'node:assert/strict';
import { calculateTargetHeight, calculateTargetRows, renderBlockMode, renderGlyphFit, renderHalfBlocks } from '../src/render.js';

test('renders upper and lower pixels as true-color foreground and background', () => {
  const output = renderHalfBlocks(Uint8Array.from([255, 0, 0, 0, 0, 255]), 1, 2);
  assert.equal(output, '\u001b[38;2;255;0;0m\u001b[48;2;0;0;255m▀\u001b[0m');
});

test('does not repeat unchanged ANSI color sequences within a row', () => {
  const output = renderHalfBlocks(Uint8Array.from([1, 2, 3, 1, 2, 3, 4, 5, 6, 4, 5, 6]), 2, 2);
  assert.equal((output.match(/38;2/g) ?? []).length, 1);
  assert.equal((output.match(/48;2/g) ?? []).length, 1);
});

test('calculates an even two-samples-per-cell height with terminal aspect correction', () => {
  assert.equal(calculateTargetHeight(1600, 900, 80), 46);
});

test('renders an HTML version without terminal escape sequences', () => {
  const output = renderHalfBlocks(Uint8Array.from([255, 0, 0, 0, 0, 255]), 1, 2, { format: 'html' });
  assert.match(output, /color:rgb\(255 0 0\)/);
  assert.match(output, /background:rgb\(0 0 255\)/);
  assert.doesNotMatch(output, /\u001b/);
});

test('quadrant mode partitions a 2 by 2 cell into independently colored areas', () => {
  const output = renderBlockMode(Uint8Array.from([255, 0, 0, 0, 0, 255, 255, 0, 0, 0, 0, 255]), 2, 2, { mode: 'quadrant' });
  assert.match(output, /▌/);
  assert.match(output, /38;2;255;0;0/);
  assert.match(output, /48;2;0;0;255/);
});

test('braille mode renders a two by four sample grid as one cell', () => {
  const pixels = Uint8Array.from(Array.from({ length: 8 }, (_, index) => index < 4 ? [255, 255, 255] : [0, 0, 0]).flat());
  const output = renderBlockMode(pixels, 2, 4, { mode: 'braille' });
  assert.match(output, /[\u2800-\u28ff]/);
  assert.equal((output.match(/\n/g) ?? []).length, 0);
});

test('row calculation is independent of renderer sample grid', () => {
  assert.equal(calculateTargetRows(1600, 900, 80), 23);
});

test('glyph fitting solves foreground and background from a coverage mask', () => {
  const masks = [{ glyph: 'X', width: 1, height: 2, alpha: [1, 0] }];
  const output = renderGlyphFit(Uint8Array.from([255, 0, 0, 0, 0, 255]), 1, 2, masks);
  assert.equal(output, '\u001b[38;2;255;0;0m\u001b[48;2;0;0;255mX\u001b[0m');
});
