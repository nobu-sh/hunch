import { describe, expect, it } from 'vitest';
import { clampPoint, pointToPosition, positionToPoint } from './overlay-position';

const VIEWPORT = { width: 1000, height: 800 };
const PANEL = { width: 300, height: 200 };

describe('positionToPoint', () => {
  it('places the panel from the top-right corner', () => {
    const position = { corner: 'top-right', offsetX: 16, offsetY: 16 } as const;

    expect(positionToPoint({ position, panel: PANEL, viewport: VIEWPORT })).toEqual({
      left: 684,
      top: 16,
    });
  });

  it('places the panel from the bottom-left corner', () => {
    const position = { corner: 'bottom-left', offsetX: 10, offsetY: 20 } as const;

    expect(positionToPoint({ position, panel: PANEL, viewport: VIEWPORT })).toEqual({
      left: 10,
      top: 580,
    });
  });

  it('clamps an offset that no longer fits a smaller viewport', () => {
    const position = { corner: 'top-left', offsetX: 900, offsetY: 700 } as const;

    expect(positionToPoint({ position, panel: PANEL, viewport: VIEWPORT })).toEqual({
      left: 700,
      top: 600,
    });
  });
});

describe('pointToPosition', () => {
  it('anchors to the nearest corner', () => {
    const position = pointToPosition({
      point: { left: 650, top: 560 },
      panel: PANEL,
      viewport: VIEWPORT,
    });

    expect(position).toEqual({ corner: 'bottom-right', offsetX: 50, offsetY: 40 });
  });

  it('round-trips through positionToPoint', () => {
    const point = { left: 120, top: 90 };
    const position = pointToPosition({ point, panel: PANEL, viewport: VIEWPORT });

    expect(positionToPoint({ position, panel: PANEL, viewport: VIEWPORT })).toEqual(point);
  });

  it('keeps the anchored corner when the viewport shrinks', () => {
    const position = pointToPosition({
      point: { left: 684, top: 16 },
      panel: PANEL,
      viewport: VIEWPORT,
    });

    const narrow = { width: 600, height: 800 };
    expect(positionToPoint({ position, panel: PANEL, viewport: narrow })).toEqual({
      left: 284,
      top: 16,
    });
  });
});

describe('clampPoint', () => {
  it('pins a panel larger than the viewport to the top-left', () => {
    const tiny = { width: 200, height: 100 };

    expect(clampPoint({ point: { left: 50, top: 50 }, panel: PANEL, viewport: tiny })).toEqual({
      left: 0,
      top: 0,
    });
  });
});
