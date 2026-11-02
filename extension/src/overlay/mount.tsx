import { flushSync } from 'react-dom';
import { createRoot } from 'react-dom/client';
import { browser } from 'wxt/browser';
import { overlayView, type OverlayView, type SessionState } from '@hunch/ui';
import { Overlay } from '@hunch/ui/components';
import OVERLAY_CSS from '@hunch/ui/overlay.css?inline';
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
import { loadOverlayPrefs, saveCollapsed, savePosition } from './prefs';

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

function wireLayout(params: { host: HTMLElement; position: OverlayPosition }): {
  applyPosition: () => void;
  dragHandle: (handle: HTMLElement | null) => (() => void) | undefined;
  dispose: () => void;
} {
  const { host } = params;
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
  // A React ref callback: React calls the returned cleanup when the handle unmounts.
  const dragHandle = (handle: HTMLElement | null): (() => void) | undefined =>
    handle === null ? undefined : makeDraggable({ handle, target: host, onMove, onDrop });

  window.addEventListener('resize', applyPosition);

  return {
    applyPosition,
    dragHandle,
    dispose: () => {
      window.removeEventListener('resize', applyPosition);
    },
  };
}

async function requestState(): Promise<SessionState | null> {
  const request: OverlayHelloRequest = { type: 'overlay-hello' };
  return browser.runtime.sendMessage<OverlayHelloRequest, SessionState | null>(request);
}

function requestClose(): void {
  const request: OverlayCloseRequest = { type: 'overlay-close' };
  void browser.runtime.sendMessage(request);
}

async function copyToClipboard(text: string): Promise<void> {
  await navigator.clipboard.writeText(text);
}

/** Mounts the overlay once per document and keeps it in sync with the background. */
export async function mountOverlay(): Promise<void> {
  const isMounted = document.documentElement.querySelector(`:scope > ${HOST_TAG}`) !== null;
  if (isMounted) return;

  const host = createHost();
  const shadow = host.attachShadow({ mode: 'closed' });
  const reactRoot = createRoot(shadow);
  // On <html>, not <body>: some sites swap out the body on navigation.
  document.documentElement.append(host);

  const prefs = await loadOverlayPrefs();
  const layout = wireLayout({ host, position: prefs.position });
  let isCollapsed = prefs.isCollapsed;
  let view: OverlayView | null = null;

  // Synchronous, so the host is measured and placed against the DOM just rendered.
  const draw = (): void => {
    if (view === null) return;
    const currentView = view;
    flushSync(() => {
      reactRoot.render(
        <>
          <style>{OVERLAY_CSS}</style>
          <Overlay
            view={currentView}
            isCollapsed={isCollapsed}
            onCollapse={() => {
              setCollapsed(true);
            }}
            onExpand={() => {
              setCollapsed(false);
            }}
            onClose={requestClose}
            onCopy={copyToClipboard}
            headerRef={layout.dragHandle}
            pillRef={layout.dragHandle}
          />
        </>,
      );
    });
    layout.applyPosition();
  };
  // Re-anchors to the same corner on the redraw, so the pill and panel share it.
  function setCollapsed(nextIsCollapsed: boolean): void {
    isCollapsed = nextIsCollapsed;
    draw();
    void saveCollapsed(isCollapsed);
  }
  const render = (state: SessionState): void => {
    view = overlayView(state);
    draw();
  };
  const unmount = (): void => {
    browser.runtime.onMessage.removeListener(onMessage);
    layout.dispose();
    reactRoot.unmount();
    host.remove();
  };
  function onMessage(message: unknown): undefined {
    if (!isOverlayMessage(message)) return;
    if (message.type === 'unmount') unmount();
    else render(message.state);
  }

  browser.runtime.onMessage.addListener(onMessage);

  const state = await requestState();
  // A stale injection into a tab that has since been turned off.
  if (state === null) {
    unmount();
    return;
  }
  render(state);
}
