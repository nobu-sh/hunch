import { describe, expect, it } from 'vitest';
import {
  anchorInArea,
  clampPoint,
  placeInArea,
  pointToPosition,
  positionToPoint,
} from './position';

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

// A work area that doesn't start at the origin, like a second monitor below a top taskbar.
const AREA = { left: 1920, top: 40, width: 1000, height: 800 };
const PILL = { width: 120, height: 32 };

describe('placeInArea', () => {
  it('offsets the position by the area origin', () => {
    const position = { corner: 'top-right', offsetX: 16, offsetY: 16 } as const;

    expect(placeInArea({ position, panel: PANEL, area: AREA })).toEqual({ left: 2604, top: 56 });
  });

  it('keeps the bottom-right corner when the panel shrinks to the pill', () => {
    const position = { corner: 'bottom-right', offsetX: 20, offsetY: 10 } as const;
    const panelAt = placeInArea({ position, panel: PANEL, area: AREA });
    const pillAt = placeInArea({ position, panel: PILL, area: AREA });

    expect(pillAt.left + PILL.width).toBe(panelAt.left + PANEL.width);
    expect(pillAt.top + PILL.height).toBe(panelAt.top + PANEL.height);
  });

  it('keeps the top-left corner when the panel shrinks to the pill', () => {
    const position = { corner: 'top-left', offsetX: 20, offsetY: 10 } as const;

    expect(placeInArea({ position, panel: PILL, area: AREA })).toEqual(
      placeInArea({ position, panel: PANEL, area: AREA }),
    );
  });

  it('clamps a position saved on a larger screen back into the area', () => {
    const position = { corner: 'top-left', offsetX: 2400, offsetY: -50 } as const;

    expect(placeInArea({ position, panel: PANEL, area: AREA })).toEqual({ left: 2620, top: 40 });
  });
});

describe('anchorInArea', () => {
  it('anchors relative to the area, not the screen origin', () => {
    const point = { left: 2600, top: 600 };

    expect(anchorInArea({ point, panel: PANEL, area: AREA })).toEqual({
      corner: 'bottom-right',
      offsetX: 20,
      offsetY: 40,
    });
  });

  it('clamps a panel dragged partly off the area', () => {
    const point = { left: 1800, top: 0 };

    expect(anchorInArea({ point, panel: PANEL, area: AREA })).toEqual({
      corner: 'top-left',
      offsetX: 0,
      offsetY: 0,
    });
  });

  it('round-trips through placeInArea', () => {
    const point = { left: 2000, top: 300 };
    const position = anchorInArea({ point, panel: PANEL, area: AREA });

    expect(placeInArea({ position, panel: PANEL, area: AREA })).toEqual(point);
  });
});
