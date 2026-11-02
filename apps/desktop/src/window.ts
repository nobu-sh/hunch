// The overlay is the whole window: it sizes the window, drags it and closes it.
import { getCurrentWindow, LogicalSize } from '@tauri-apps/api/window';

/** How far a press moves before it's a drag rather than a click. */
const DRAG_THRESHOLD_PX = 4;
const PRIMARY_BUTTON = 0;

function reportFailure(action: string) {
  return (error: unknown) => {
    console.error(`Hunch couldn't ${action} the window:`, error);
  };
}

/** Keeps the window the size of `element`. Returns the cleanup. */
export function fitWindowTo(element: HTMLElement): () => void {
  const observer = new ResizeObserver(() => {
    const { width, height } = element.getBoundingClientRect();
    if (width === 0 || height === 0) return;
    getCurrentWindow()
      .setSize(new LogicalSize(Math.ceil(width), Math.ceil(height)))
      .catch(reportFailure('resize'));
  });
  observer.observe(element);
  return () => {
    observer.disconnect();
  };
}

/**
 * A React ref callback that moves the window when `handle` is dragged. Presses on buttons inside
 * the handle stay clicks, and so does a press on a button handle that doesn't move.
 */
export function dragWindowFrom(handle: HTMLElement | null): (() => void) | undefined {
  if (handle === null) return undefined;

  let pressedAt: { x: number; y: number } | null = null;
  const onPointerDown = (event: PointerEvent) => {
    const button = event.target instanceof Element ? event.target.closest('button') : null;
    const isInnerButton = button !== null && button !== handle;
    if (event.button !== PRIMARY_BUTTON || isInnerButton) return;
    pressedAt = { x: event.clientX, y: event.clientY };
  };
  const onPointerMove = (event: PointerEvent) => {
    if (pressedAt === null) return;
    const distance = Math.hypot(event.clientX - pressedAt.x, event.clientY - pressedAt.y);
    if (distance < DRAG_THRESHOLD_PX) return;
    pressedAt = null;
    getCurrentWindow().startDragging().catch(reportFailure('drag'));
  };
  const onPointerUp = () => {
    pressedAt = null;
  };

  handle.addEventListener('pointerdown', onPointerDown);
  handle.addEventListener('pointermove', onPointerMove);
  handle.addEventListener('pointerup', onPointerUp);
  return () => {
    handle.removeEventListener('pointerdown', onPointerDown);
    handle.removeEventListener('pointermove', onPointerMove);
    handle.removeEventListener('pointerup', onPointerUp);
  };
}

export function closeWindow(): void {
  getCurrentWindow().close().catch(reportFailure('close'));
}
