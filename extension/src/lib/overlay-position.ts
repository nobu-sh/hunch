export type Corner = 'top-left' | 'top-right' | 'bottom-left' | 'bottom-right';

/** Where the overlay sits, as offsets from its nearest viewport corner so it survives resizes. */
export interface OverlayPosition {
  corner: Corner;
  offsetX: number;
  offsetY: number;
}

export interface Size {
  width: number;
  height: number;
}

export interface Point {
  left: number;
  top: number;
}

export const DEFAULT_POSITION: OverlayPosition = { corner: 'top-right', offsetX: 16, offsetY: 16 };

function clamp(params: { value: number; max: number }): number {
  return Math.min(Math.max(params.value, 0), Math.max(params.max, 0));
}

/** Keeps the panel fully inside the viewport (or pinned to its top-left if it can't fit). */
export function clampPoint(params: { point: Point; panel: Size; viewport: Size }): Point {
  const { point, panel, viewport } = params;
  return {
    left: clamp({ value: point.left, max: viewport.width - panel.width }),
    top: clamp({ value: point.top, max: viewport.height - panel.height }),
  };
}

/** Top-left of the panel for a stored position, clamped to the viewport. */
export function positionToPoint(params: {
  position: OverlayPosition;
  panel: Size;
  viewport: Size;
}): Point {
  const { position, panel, viewport } = params;
  const isRight = position.corner.endsWith('right');
  const isBottom = position.corner.startsWith('bottom');
  const point = {
    left: isRight ? viewport.width - panel.width - position.offsetX : position.offsetX,
    top: isBottom ? viewport.height - panel.height - position.offsetY : position.offsetY,
  };
  return clampPoint({ point, panel, viewport });
}

/** Anchors a dropped panel to the corner nearest its center. */
export function pointToPosition(params: {
  point: Point;
  panel: Size;
  viewport: Size;
}): OverlayPosition {
  const { panel, viewport } = params;
  const point = clampPoint(params);
  const isRight = point.left + panel.width / 2 > viewport.width / 2;
  const isBottom = point.top + panel.height / 2 > viewport.height / 2;
  return {
    corner: `${isBottom ? 'bottom' : 'top'}-${isRight ? 'right' : 'left'}`,
    offsetX: isRight ? viewport.width - panel.width - point.left : point.left,
    offsetY: isBottom ? viewport.height - panel.height - point.top : point.top,
  };
}
