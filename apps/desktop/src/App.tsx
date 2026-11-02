import { useEffect, useRef, useState } from 'react';
import { flushSync } from 'react-dom';
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
  PinButton,
  SourcePicker,
  SourcesButton,
  useSourcePicker,
  type CloseLabels,
} from '@hunch/ui/components';
import { backend } from './backend';
import { anchorWindowTo, type WindowAnchor } from './placement';
import { followSession } from './session';
import { useAlwaysOnTop } from './use-always-on-top';
import { useSubscription } from './use-subscription';
import { dragWindowFrom } from './window';

/** The app listens to one source at a time, so its session needs no real id yet. */
const SESSION_ID = 0;
// The tray menu quits, so closing only hides the window and its session keeps running.
const HIDE_LABELS: CloseLabels = { label: 'Hide', title: 'Hide to tray' };

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
  const alwaysOnTop = useAlwaysOnTop(showError);

  const anchorRef = useRef<WindowAnchor | null>(null);
  useEffect(() => {
    if (rootRef.current === null) return;
    const anchor = anchorWindowTo(rootRef.current);
    anchorRef.current = anchor;
    return () => {
      anchor.stop();
      anchorRef.current = null;
    };
  }, []);
  // The panel and pill differ most in size, so swapping them is resized out of sight.
  const setCollapsed = (collapsed: boolean) => {
    const render = () => {
      flushSync(() => {
        setIsCollapsed(collapsed);
      });
    };
    const anchor = anchorRef.current;
    if (anchor === null) render();
    else anchor.resizeOutOfSight(render).catch(showError);
  };

  const statusSource = picker.current?.id === status?.sourceId ? picker.current : null;
  const notice = status && sourceNotice({ status, source: statusSource });

  return (
    <div ref={rootRef}>
      <Overlay
        view={overlayView(state)}
        isCollapsed={isCollapsed}
        onCollapse={() => {
          setCollapsed(true);
        }}
        onExpand={() => {
          setCollapsed(false);
        }}
        onClose={() => {
          backend.hideToTray().catch(showError);
        }}
        closeLabels={HIDE_LABELS}
        onCopy={copyToClipboard}
        headerRef={dragWindowFrom}
        pillRef={dragWindowFrom}
        headerActions={
          <>
            <PinButton isPinned={alwaysOnTop.isOnTop} onToggle={alwaysOnTop.setOnTop} />
            <SourcesButton
              isOpen={picker.isOpen}
              onClick={picker.isOpen ? picker.close : picker.open}
            />
          </>
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
