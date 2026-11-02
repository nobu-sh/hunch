// Keeps the window anchored to a corner of its monitor's work area: it restores the saved spot,
// resizes around that corner, and remembers where a drag leaves it.
import {
  anchorInArea,
  DEFAULT_POSITION,
  placeInArea,
  type Area,
  type Corner,
  type OverlayPosition,
  type Size,
} from '@hunch/ui';
import type { UnlistenFn } from '@tauri-apps/api/event';
import {
  availableMonitors,
  currentMonitor,
  getCurrentWindow,
  primaryMonitor,
  type Monitor,
} from '@tauri-apps/api/window';
import { backend, type SavedPosition, type WindowBounds } from './backend';

/** How long the window stays still after a move before it counts as dropped. */
const DROP_SETTLE_MS = 300;
/** The longest a resize stays cloaked if the page never reaches its new size. */
const CLOAK_TIMEOUT_MS = 250;

/** Keeps the window placed; `stop` is the cleanup. */
export interface WindowAnchor {
  stop: () => void;
  /**
   * Runs `render`, which must update the DOM synchronously, and resizes to it with the window
   * cloaked, so the old and new sizes swap in one frame instead of showing bare blur.
   */
  resizeOutOfSight: (render: () => void) => Promise<void>;
}

interface Anchor {
  monitor: Monitor;
  position: OverlayPosition;
}

/** The monitor's work area in logical pixels, so offsets survive a scale change. */
function workArea(monitor: Monitor): Area {
  const { position, size } = monitor.workArea;
  const scale = monitor.scaleFactor;
  return {
    left: position.x / scale,
    top: position.y / scale,
    width: size.width / scale,
    height: size.height / scale,
  };
}

/**
 * The panel rounded up to whole physical pixels, back in logical ones, so its far edges land on
 * the same pixel whichever size it has.
 */
function wholePixels({ panel, scale }: { panel: Size; scale: number }): Size {
  return {
    width: Math.ceil(panel.width * scale) / scale,
    height: Math.ceil(panel.height * scale) / scale,
  };
}

function boundsFor({ anchor, panel }: { anchor: Anchor; panel: Size }): WindowBounds {
  const scale = anchor.monitor.scaleFactor;
  const size = wholePixels({ panel, scale });
  const point = placeInArea({
    position: anchor.position,
    panel: size,
    area: workArea(anchor.monitor),
  });
  return {
    x: Math.round(point.left * scale),
    y: Math.round(point.top * scale),
    width: Math.round(size.width * scale),
    height: Math.round(size.height * scale),
  };
}

/** The saved spot on its monitor, or on the primary monitor if that one is gone. */
async function restoreAnchor(saved: SavedPosition | null): Promise<Anchor> {
  const monitors = await availableMonitors();
  const savedMonitor = monitors.find((monitor) => monitor.name === saved?.monitor);
  const monitor = savedMonitor ?? (await primaryMonitor()) ?? monitors[0];
  if (monitor === undefined) throw new Error('there is no monitor to show the window on');
  const position = saved
    ? { corner: saved.corner, offsetX: saved.offsetX, offsetY: saved.offsetY }
    : DEFAULT_POSITION;
  return { monitor, position };
}

/** Lines the page up with the anchored corner, so a resize grows and shrinks from it. */
function showCorner(corner: Corner): void {
  document.documentElement.dataset.corner = corner;
}

function measure(element: HTMLElement): Size | null {
  const { width, height } = element.getBoundingClientRect();
  return width === 0 || height === 0 ? null : { width, height };
}

/** Resolves once the page has painted at the size of `bounds`, or after the timeout. */
function paintedAt(bounds: WindowBounds): Promise<void> {
  const hasSize = () =>
    Math.round(window.innerWidth * window.devicePixelRatio) === bounds.width &&
    Math.round(window.innerHeight * window.devicePixelRatio) === bounds.height;
  return new Promise((resolve) => {
    const finish = () => {
      clearTimeout(timer);
      window.removeEventListener('resize', check);
      resolve();
    };
    const timer = setTimeout(finish, CLOAK_TIMEOUT_MS);
    // A frame's callbacks run before it paints, so the second one follows the painted frame.
    const afterNextPaint = () => {
      requestAnimationFrame(() => requestAnimationFrame(finish));
    };
    function check() {
      if (!hasSize()) return;
      window.removeEventListener('resize', check);
      afterNextPaint();
    }
    window.addEventListener('resize', check);
    check();
  });
}

function reportFailure(action: string) {
  return (error: unknown) => {
    console.error(`Hunch couldn't ${action} the window:`, error);
  };
}

/**
 * Places the window at its saved spot, shows it, and keeps it the size of `element` around its
 * anchored corner.
 */
export function anchorWindowTo(element: HTMLElement): WindowAnchor {
  const appWindow = getCurrentWindow();
  let anchor: Anchor | null = null;
  // Where the client area was last put, to tell the window's own moves from the user's.
  let placedAt: { x: number; y: number } | null = null;
  let dropTimer: ReturnType<typeof setTimeout> | undefined;
  let stopFollowingMoves: UnlistenFn | undefined;
  // Aborted by the cleanup, which can run while `start` awaits.
  const disposal = new AbortController();
  const isDisposed = () => disposal.signal.aborted;

  const place = async (): Promise<WindowBounds | null> => {
    const panel = measure(element);
    if (anchor === null || panel === null) return null;
    const bounds = boundsFor({ anchor, panel });
    placedAt = { x: bounds.x, y: bounds.y };
    await backend.setWindowBounds(bounds);
    return bounds;
  };

  const resizeOutOfSight = async (render: () => void) => {
    if (anchor === null) {
      render();
      return;
    }
    await backend.setWindowCloaked(true);
    try {
      render();
      const bounds = await place();
      if (bounds !== null) await paintedAt(bounds);
    } finally {
      await backend.setWindowCloaked(false);
    }
  };

  const drop = async () => {
    const panel = measure(element);
    const monitor = (await currentMonitor()) ?? anchor?.monitor;
    if (panel === null || monitor === undefined) return;
    const client = await appWindow.innerPosition();
    const isWherePlaced = client.x === placedAt?.x && client.y === placedAt.y;
    if (isWherePlaced) return;
    const scale = monitor.scaleFactor;
    const point = { left: client.x / scale, top: client.y / scale };
    const size = wholePixels({ panel, scale });
    const position = anchorInArea({ point, panel: size, area: workArea(monitor) });
    anchor = { monitor, position };
    showCorner(position.corner);
    await backend.saveWindowPosition({ monitor: monitor.name, ...position });
  };

  const observer = new ResizeObserver(() => {
    place().catch(reportFailure('resize'));
  });

  // Moves report the outer position, so `drop` compares the client position once they settle.
  const followMoves = () =>
    appWindow.onMoved(() => {
      clearTimeout(dropTimer);
      dropTimer = setTimeout(() => {
        drop().catch(reportFailure('remember where you left'));
      }, DROP_SETTLE_MS);
    });

  const start = async () => {
    try {
      anchor = await restoreAnchor((await backend.windowSettings()).position);
      showCorner(anchor.position.corner);
      await place();
    } finally {
      // Shown even if placing failed, so the app is never invisible.
      await appWindow.show();
    }
    if (isDisposed()) return;
    observer.observe(element);
    const stop = await followMoves();
    if (isDisposed()) stop();
    else stopFollowingMoves = stop;
  };
  start().catch(reportFailure('place'));

  const stop = () => {
    disposal.abort();
    observer.disconnect();
    clearTimeout(dropTimer);
    stopFollowingMoves?.();
  };
  return { stop, resizeOutOfSight };
}
