import type { Point } from '@/lib/overlay-position';

/** Pointer travel that turns a press into a drag rather than a click. */
const DRAG_THRESHOLD_PX = 4;

/**
 * Moves `target` while `handle` is dragged. Presses on buttons inside the handle stay clicks;
 * a press on the handle that never moves calls `onClick`.
 */
export function makeDraggable(params: {
  handle: HTMLElement;
  target: HTMLElement;
  onMove: (point: Point) => void;
  onDrop: () => void;
  onClick?: () => void;
}): void {
  const { handle, target } = params;

  handle.addEventListener('pointerdown', (event) => {
    const pressedButton = (event.target as Element).closest('button');
    const isInnerButton = pressedButton !== null && pressedButton !== handle;
    if (event.button !== 0 || isInnerButton) return;

    const start = { x: event.clientX, y: event.clientY };
    const origin = target.getBoundingClientRect();
    let isDragging = false;
    handle.setPointerCapture(event.pointerId);

    const onPointerMove = (move: PointerEvent): void => {
      const dx = move.clientX - start.x;
      const dy = move.clientY - start.y;
      isDragging ||= Math.hypot(dx, dy) >= DRAG_THRESHOLD_PX;
      if (isDragging) params.onMove({ left: origin.left + dx, top: origin.top + dy });
    };
    const onPointerUp = (): void => {
      handle.removeEventListener('pointermove', onPointerMove);
      handle.removeEventListener('pointerup', onPointerUp);
      handle.removeEventListener('pointercancel', onPointerUp);
      if (isDragging) params.onDrop();
      else params.onClick?.();
    };

    handle.addEventListener('pointermove', onPointerMove);
    handle.addEventListener('pointerup', onPointerUp);
    handle.addEventListener('pointercancel', onPointerUp);
  });
}
