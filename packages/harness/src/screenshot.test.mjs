import { describe, expect, it } from 'vitest';
import { paddedClip } from './screenshot.mjs';

const viewport = { width: 1000, height: 800 };

describe('paddedClip', () => {
  it('adds the margin on every side', () => {
    const box = { x: 100, y: 200, width: 300, height: 150 };
    expect(paddedClip({ box, margin: 10, viewport })).toEqual({
      x: 90,
      y: 190,
      width: 320,
      height: 170,
    });
  });

  it('stops at the viewport edges', () => {
    const box = { x: 5, y: 700, width: 990, height: 95 };
    expect(paddedClip({ box, margin: 16, viewport })).toEqual({
      x: 0,
      y: 684,
      width: 1000,
      height: 116,
    });
  });
});
