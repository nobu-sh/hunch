import { useEffect, useRef, useState } from 'react';
import { createSessionState, overlayView, withError, type SessionState } from '@hunch/ui';
import { Overlay } from '@hunch/ui/components';
import { followSession } from './session';
import { closeWindow, dragWindowFrom, fitWindowTo } from './window';

/** The app listens to one source at a time, so its session needs no real id yet. */
const SOURCE_ID = 0;

async function copyToClipboard(text: string): Promise<void> {
  await navigator.clipboard.writeText(text);
}

export function App() {
  const [state, setState] = useState<SessionState>(() => createSessionState(SOURCE_ID));
  const [isCollapsed, setIsCollapsed] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    let unfollow: (() => void) | null = null;
    let isCancelled = false;
    followSession(setState)
      .then((unlisten) => {
        if (isCancelled) unlisten();
        else unfollow = unlisten;
      })
      .catch((error: unknown) => {
        setState((current) => withError({ state: current, error: String(error) }));
      });
    return () => {
      isCancelled = true;
      unfollow?.();
    };
  }, []);

  useEffect(() => {
    if (rootRef.current === null) return;
    return fitWindowTo(rootRef.current);
  }, []);

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
        onCopy={copyToClipboard}
        headerRef={dragWindowFrom}
        pillRef={dragWindowFrom}
      />
    </div>
  );
}
