import type { Point } from '@hunch/ui';

/** Pointer travel that turns a press into a drag rather than a click. */
const DRAG_THRESHOLD_PX = 4;

/**
 * Moves `target` while `handle` is dragged. Presses on buttons inside the handle stay clicks,
 * and the click that ends a drag is swallowed so the handle's own click handler doesn't fire.
 * Returns a function that removes the listeners.
 */
export function makeDraggable(params: {
  handle: HTMLElement;
  target: HTMLElement;
  onMove: (point: Point) => void;
  onDrop: () => void;
}): () => void {
  const { handle, target } = params;
  let isSwallowingClick = false;

  const onPointerDown = (event: PointerEvent): void => {
    isSwallowingClick = false;
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
      if (!isDragging) return;
      isSwallowingClick = true;
      params.onDrop();
    };

    handle.addEventListener('pointermove', onPointerMove);
    handle.addEventListener('pointerup', onPointerUp);
    handle.addEventListener('pointercancel', onPointerUp);
  };
  // Stopped at the handle, so it never bubbles up to React's listener on the root.
  const onClick = (event: MouseEvent): void => {
    if (!isSwallowingClick) return;
    isSwallowingClick = false;
    event.stopPropagation();
  };

  handle.addEventListener('pointerdown', onPointerDown);
  handle.addEventListener('click', onClick);
  return () => {
    handle.removeEventListener('pointerdown', onPointerDown);
    handle.removeEventListener('click', onClick);
  };
}
