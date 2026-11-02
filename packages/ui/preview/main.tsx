import { useState } from 'react';
import { createRoot } from 'react-dom/client';
import overlayCss from '../src/styles/overlay.css?inline';
import { Overlay } from '../src/components';
import {
  ANALYZING_VIEW,
  ERROR_VIEW,
  LISTENING_VIEW,
  LOCKED_VIEW,
} from '../src/components/fixtures';
import type { OverlayView } from '../src/overlay-view';

const PREVIEWS: { name: string; view: OverlayView; isCollapsed?: boolean }[] = [
  { name: 'listening', view: LISTENING_VIEW },
  { name: 'analyzing', view: ANALYZING_VIEW },
  { name: 'locked', view: LOCKED_VIEW },
  { name: 'error', view: ERROR_VIEW },
  { name: 'collapsed', view: LOCKED_VIEW, isCollapsed: true },
];

function PreviewOverlay(props: { view: OverlayView; isCollapsed: boolean }) {
  const [isCollapsed, setIsCollapsed] = useState(props.isCollapsed);
  return (
    <Overlay
      view={props.view}
      isCollapsed={isCollapsed}
      onCollapse={() => {
        setIsCollapsed(true);
      }}
      onExpand={() => {
        setIsCollapsed(false);
      }}
      onClose={() => {
        console.info('Hunch preview: close');
      }}
      onCopy={(text) => navigator.clipboard.writeText(text)}
    />
  );
}

// Each preview sits in its own shadow root, like the extension's overlay, so `:host` tokens apply.
function mountPreview(params: {
  container: HTMLElement;
  name: string;
  view: OverlayView;
  isCollapsed: boolean;
}) {
  const figure = document.createElement('figure');
  const caption = document.createElement('figcaption');
  caption.textContent = params.name;
  const host = document.createElement('div');
  figure.append(caption, host);
  params.container.append(figure);

  const shadow = host.attachShadow({ mode: 'open' });
  const style = document.createElement('style');
  style.textContent = overlayCss;
  const mountPoint = document.createElement('div');
  shadow.append(style, mountPoint);
  createRoot(mountPoint).render(
    <PreviewOverlay view={params.view} isCollapsed={params.isCollapsed} />,
  );
}

const container = document.getElementById('previews');
if (!container) throw new Error('Hunch preview page is missing #previews');
for (const preview of PREVIEWS) {
  mountPreview({ container, ...preview, isCollapsed: preview.isCollapsed ?? false });
}
