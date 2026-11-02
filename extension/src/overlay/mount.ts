import { browser } from 'wxt/browser';
import { overlayView, type SessionState } from '@hunch/ui';
import {
  isOverlayMessage,
  type OverlayCloseRequest,
  type OverlayHelloRequest,
} from '@/lib/messages';
import {
  pointToPosition,
  positionToPoint,
  clampPoint,
  type OverlayPosition,
  type Point,
  type Size,
} from '@/lib/overlay-position';
import { makeDraggable } from './drag';
import { createPanel, type Panel } from './panel';
import { loadOverlayPrefs, saveCollapsed, savePosition } from './prefs';
import { OVERLAY_CSS } from './styles';

const HOST_TAG = 'hunch-overlay';
const TOP_Z_INDEX = '2147483647';

function viewportSize(): Size {
  return { width: document.documentElement.clientWidth, height: window.innerHeight };
}

function elementSize(element: HTMLElement): Size {
  const rect = element.getBoundingClientRect();
  return { width: rect.width, height: rect.height };
}

/** Fixed-position host; inline `!important` so page CSS can't move or hide it. */
function createHost(): HTMLElement {
  const host = document.createElement(HOST_TAG);
  const styles: Record<string, string> = {
    all: 'initial',
    position: 'fixed',
    'z-index': TOP_Z_INDEX,
    left: '0px',
    top: '0px',
    display: 'block',
    // Shown by the first `placeHost`, once its stored position is known.
    visibility: 'hidden',
  };
  for (const [property, value] of Object.entries(styles)) {
    host.style.setProperty(property, value, 'important');
  }
  return host;
}

function placeHost(params: { host: HTMLElement; point: Point }): void {
  params.host.style.setProperty('left', `${String(params.point.left)}px`, 'important');
  params.host.style.setProperty('top', `${String(params.point.top)}px`, 'important');
  params.host.style.setProperty('visibility', 'visible', 'important');
}

function wireLayout(params: {
  host: HTMLElement;
  panel: Panel;
  position: OverlayPosition;
  isCollapsed: boolean;
}): { applyPosition: () => void; dispose: () => void } {
  const { host, panel } = params;
  let position = params.position;

  const applyPosition = (): void => {
    placeHost({
      host,
      point: positionToPoint({ position, panel: elementSize(host), viewport: viewportSize() }),
    });
  };
  const onMove = (point: Point): void => {
    placeHost({
      host,
      point: clampPoint({ point, panel: elementSize(host), viewport: viewportSize() }),
    });
  };
  const onDrop = (): void => {
    const rect = host.getBoundingClientRect();
    position = pointToPosition({
      point: { left: rect.left, top: rect.top },
      panel: elementSize(host),
      viewport: viewportSize(),
    });
    void savePosition(position);
  };
  const setCollapsed = (isCollapsed: boolean): void => {
    panel.setCollapsed(isCollapsed);
    // Re-anchors to the same corner, so the pill and panel share it.
    applyPosition();
    void saveCollapsed(isCollapsed);
  };

  panel.setCollapsed(params.isCollapsed);
  makeDraggable({ handle: panel.header, target: host, onMove, onDrop });
  makeDraggable({
    handle: panel.pill,
    target: host,
    onMove,
    onDrop,
    onClick: () => {
      setCollapsed(false);
    },
  });
  panel.collapseButton.addEventListener('click', () => {
    setCollapsed(true);
  });
  window.addEventListener('resize', applyPosition);

  return {
    applyPosition,
    dispose: () => {
      window.removeEventListener('resize', applyPosition);
    },
  };
}

async function requestState(): Promise<SessionState | null> {
  const request: OverlayHelloRequest = { type: 'overlay-hello' };
  return browser.runtime.sendMessage<OverlayHelloRequest, SessionState | null>(request);
}

/** Mounts the overlay once per document and keeps it in sync with the background. */
export async function mountOverlay(): Promise<void> {
  const isMounted = document.documentElement.querySelector(`:scope > ${HOST_TAG}`) !== null;
  if (isMounted) return;

  const host = createHost();
  const shadow = host.attachShadow({ mode: 'closed' });
  const style = document.createElement('style');
  style.textContent = OVERLAY_CSS;
  const panel = createPanel();
  shadow.append(style, panel.root);
  // On <html>, not <body>: some sites swap out the body on navigation.
  document.documentElement.append(host);

  const prefs = await loadOverlayPrefs();
  const layout = wireLayout({ host, panel, ...prefs });

  const render = (state: SessionState): void => {
    panel.render(overlayView(state));
    layout.applyPosition();
  };
  const unmount = (): void => {
    browser.runtime.onMessage.removeListener(onMessage);
    layout.dispose();
    host.remove();
  };
  function onMessage(message: unknown): undefined {
    if (!isOverlayMessage(message)) return;
    if (message.type === 'unmount') unmount();
    else render(message.state);
  }

  panel.closeButton.addEventListener('click', () => {
    const request: OverlayCloseRequest = { type: 'overlay-close' };
    void browser.runtime.sendMessage(request);
  });
  browser.runtime.onMessage.addListener(onMessage);

  const state = await requestState();
  // A stale injection into a tab that has since been turned off.
  if (state === null) {
    unmount();
    return;
  }
  render(state);
}
