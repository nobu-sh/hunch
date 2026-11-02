import { useState, type ReactNode } from 'react';
import { createRoot } from 'react-dom/client';
import overlayCss from '../src/styles/overlay.css?inline';
import {
  Notice,
  Overlay,
  SourcePicker,
  SourcesButton,
  useSourcePicker,
  type SourceClient,
  type SourcePickerProps,
} from '../src/components';
import {
  ANALYZING_VIEW,
  DEVICES_ONLY_LIST,
  ENDED_STATUS,
  ERROR_VIEW,
  FAILED_STATUS,
  HEADSET_SOURCE,
  LISTENING_VIEW,
  LOCKED_VIEW,
  SILENT_STATUS,
  SOURCE_LIST,
  SPEAKERS_SOURCE,
  SPOTIFY_SOURCE,
} from '../src/components/fixtures';
import type { OverlayView } from '../src/overlay-view';
import { sourceNotice, type SourceStatus } from '../src/sources';

/** How long the mock client takes, so loading and switching show. */
const MOCK_DELAY_MS = 600;
/** The mock client refuses this source, to show a failed switch. */
const REFUSED_SOURCE_ID = HEADSET_SOURCE.id;

const PICKER_DEFAULTS: SourcePickerProps = {
  sources: SOURCE_LIST,
  currentId: SPEAKERS_SOURCE.id,
  pendingId: null,
  error: null,
  onSelect: (id) => {
    console.info('Hunch preview: select', id);
  },
  onBack: () => {
    console.info('Hunch preview: back');
  },
};

interface Preview {
  name: string;
  view: OverlayView;
  isCollapsed?: boolean;
  /** A source status shown as a notice. */
  status?: SourceStatus;
  /** A fixed picker state, or a picker run against the mock client. */
  picker?: Partial<SourcePickerProps> | 'interactive';
}

const PREVIEWS: Preview[] = [
  { name: 'listening', view: LISTENING_VIEW },
  { name: 'analyzing', view: ANALYZING_VIEW },
  { name: 'locked', view: LOCKED_VIEW },
  { name: 'error', view: ERROR_VIEW },
  { name: 'collapsed', view: LOCKED_VIEW, isCollapsed: true },
  { name: 'picker (interactive, headset fails)', view: LOCKED_VIEW, picker: 'interactive' },
  { name: 'picker: apps tab', view: ANALYZING_VIEW, picker: { initialTab: 'app' } },
  { name: 'picker: devices tab', view: ANALYZING_VIEW, picker: { initialTab: 'device' } },
  { name: 'picker: devices only', view: ANALYZING_VIEW, picker: { sources: DEVICES_ONLY_LIST } },
  { name: 'picker: loading', view: LISTENING_VIEW, picker: { sources: null } },
  {
    name: 'picker: switching',
    view: LISTENING_VIEW,
    picker: { initialTab: 'app', pendingId: SPOTIFY_SOURCE.id },
  },
  {
    name: 'picker: switch failed',
    view: LISTENING_VIEW,
    picker: {
      initialTab: 'device',
      error: "Couldn't switch: The device is in use by another app",
    },
  },
  {
    name: 'picker: backend missing',
    view: LISTENING_VIEW,
    picker: {
      sources: { sources: [], listsApps: false },
      currentId: null,
      error: "Couldn't list sources: Command list_sources not found",
    },
  },
  { name: 'notice: app closed', view: LOCKED_VIEW, status: ENDED_STATUS },
  { name: 'notice: silent', view: LISTENING_VIEW, status: SILENT_STATUS },
  { name: 'notice: failed', view: ERROR_VIEW, status: FAILED_STATUS },
];

function wait(): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, MOCK_DELAY_MS));
}

const MOCK_CLIENT: SourceClient = {
  listSources: () => wait().then(() => SOURCE_LIST),
  currentSource: () => Promise.resolve(SPEAKERS_SOURCE),
  selectSource: async (id) => {
    await wait();
    if (id === REFUSED_SOURCE_ID) throw new Error('The device is in use by another app');
  },
};

function statusNotice(status: SourceStatus): ReactNode {
  const source = SOURCE_LIST.sources.find(({ id }) => id === status.sourceId) ?? null;
  const notice = sourceNotice({ status, source });
  return notice && <Notice {...notice} />;
}

function PreviewOverlay(props: { preview: Preview; body?: ReactNode; headerActions?: ReactNode }) {
  const { preview } = props;
  const [isCollapsed, setIsCollapsed] = useState(preview.isCollapsed ?? false);
  return (
    <Overlay
      view={preview.view}
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
      notice={preview.status && statusNotice(preview.status)}
      headerActions={props.headerActions}
      body={props.body}
    />
  );
}

function InteractivePickerPreview({ preview }: { preview: Preview }) {
  const picker = useSourcePicker({ client: MOCK_CLIENT });
  return (
    <PreviewOverlay
      preview={preview}
      headerActions={
        <SourcesButton
          isOpen={picker.isOpen}
          onClick={picker.isOpen ? picker.close : picker.open}
        />
      }
      body={picker.isOpen ? <SourcePicker {...picker.pickerProps} /> : undefined}
    />
  );
}

function PreviewEntry({ preview }: { preview: Preview }) {
  if (preview.picker === 'interactive') return <InteractivePickerPreview preview={preview} />;
  const button = <SourcesButton isOpen={preview.picker !== undefined} onClick={() => undefined} />;
  const body = preview.picker && <SourcePicker {...PICKER_DEFAULTS} {...preview.picker} />;
  return <PreviewOverlay preview={preview} headerActions={button} body={body} />;
}

// Each preview sits in its own shadow root, like the extension's overlay, so `:host` tokens apply.
function mountPreview(params: { container: HTMLElement; preview: Preview }) {
  const figure = document.createElement('figure');
  const caption = document.createElement('figcaption');
  caption.textContent = params.preview.name;
  const host = document.createElement('div');
  figure.append(caption, host);
  params.container.append(figure);

  const shadow = host.attachShadow({ mode: 'open' });
  const style = document.createElement('style');
  style.textContent = overlayCss;
  const mountPoint = document.createElement('div');
  shadow.append(style, mountPoint);
  createRoot(mountPoint).render(<PreviewEntry preview={params.preview} />);
}

const container = document.getElementById('previews');
if (!container) throw new Error('Hunch preview page is missing #previews');
for (const preview of PREVIEWS) {
  mountPreview({ container, preview });
}
