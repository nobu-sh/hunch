import { useEffect, useRef, useState } from 'react';
import {
  applySourceStatus,
  createSessionState,
  errorMessage,
  overlayView,
  sourceNotice,
  withError,
  withFreshSession,
  type SessionState,
  type SourceStatus,
} from '@hunch/ui';
import {
  Notice,
  Overlay,
  SourcePicker,
  SourcesButton,
  useSourcePicker,
  type CloseLabels,
} from '@hunch/ui/components';
import { backend } from './backend';
import { followSession } from './session';
import { useSubscription } from './use-subscription';
import { closeWindow, dragWindowFrom, fitWindowTo } from './window';

/** The app listens to one source at a time, so its session needs no real id yet. */
const SESSION_ID = 0;
const QUIT_LABELS: CloseLabels = { label: 'Quit', title: 'Quit Hunch' };

async function copyToClipboard(text: string): Promise<void> {
  await navigator.clipboard.writeText(text);
}

export function App() {
  const [state, setState] = useState<SessionState>(() => createSessionState(SESSION_ID));
  const [status, setStatus] = useState<SourceStatus | null>(null);
  const [isCollapsed, setIsCollapsed] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);
  const picker = useSourcePicker({
    client: backend,
    // The backend starts a fresh session on the new source, after the old one's last window ends.
    onSwitch: () => {
      setState(withFreshSession);
      setStatus(null);
    },
  });

  const showError = (error: unknown) => {
    setState((current) => withError({ state: current, error: errorMessage(error) }));
  };
  useSubscription({
    subscribe: () => followSession({ backend, onChange: setState }),
    onError: showError,
  });
  const followStatus = (next: SourceStatus) => {
    setStatus(next);
    // `listen` may have started a source the picker hasn't heard of, so its notices can name it.
    if (next.state === 'listening') picker.refreshCurrent();
    setState((current) => applySourceStatus({ state: current, sourceState: next.state }));
  };
  useSubscription({ subscribe: () => backend.onSourceStatus(followStatus), onError: showError });

  useEffect(() => {
    if (rootRef.current === null) return;
    return fitWindowTo(rootRef.current);
  }, []);

  const statusSource = picker.current?.id === status?.sourceId ? picker.current : null;
  const notice = status && sourceNotice({ status, source: statusSource });

  return (
    <div ref={rootRef}>
      <Overlay
        view={overlayView(state)}
        isCollapsed={isCollapsed}
        onCollapse={() => {
          setIsCollapsed(true);
        }}
        onExpand={() => {
          setIsCollapsed(false);
        }}
        onClose={closeWindow}
        closeLabels={QUIT_LABELS}
        onCopy={copyToClipboard}
        headerRef={dragWindowFrom}
        pillRef={dragWindowFrom}
        headerActions={
          <SourcesButton
            isOpen={picker.isOpen}
            onClick={picker.isOpen ? picker.close : picker.open}
          />
        }
        notice={
          notice && (
            <Notice
              {...notice}
              onDismiss={() => {
                setStatus(null);
              }}
            />
          )
        }
        body={picker.isOpen ? <SourcePicker {...picker.pickerProps} /> : undefined}
      />
    </div>
  );
}
