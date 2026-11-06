// Dev builds publish what the UI holds on `window.__hunchDev`, so the browser mixins read the
// session's data instead of scraping the DOM. Callers guard every use with
// `import.meta.env.DEV`, which keeps this out of release bundles.
import { createSessionState, type SessionState, type SourceStatus } from '@hunch/ui';
import {
  DESKTOP_DEV_STATE_GLOBAL,
  DEV_LOG_LIMIT,
  type DesktopDevState,
  type DevLogEntry,
  type DevLogMessage,
} from '@hunch/ui/dev';
import type { Backend } from './backend';

/** Replaced by the overlay's own state as soon as it renders. */
const PLACEHOLDER_SESSION_ID = 0;

const devState: DesktopDevState = {
  state: createSessionState(PLACEHOLDER_SESSION_ID),
  source: null,
  log: [],
};
let nextSeq = 1;

function record(message: DevLogMessage): void {
  const entry: DevLogEntry = { ...message, seq: nextSeq, receivedAt: Date.now() };
  nextSeq += 1;
  devState.log = [...devState.log, entry].slice(-DEV_LOG_LIMIT);
}

/**
 * Puts the dev state on `window.__hunchDev` and wraps the backend so every session event and
 * source status lands in its log before the UI handles it.
 */
export function exposeDevState(backend: Backend): Backend {
  Object.assign(window, { [DESKTOP_DEV_STATE_GLOBAL]: devState });
  return {
    ...backend,
    onSessionEvent: (handler) =>
      backend.onSessionEvent((event) => {
        record({ kind: 'sessionEvent', event });
        handler(event);
      }),
    onSourceStatus: (handler) =>
      backend.onSourceStatus((status) => {
        record({ kind: 'sourceStatus', status });
        handler(status);
      }),
  };
}

/** Publishes the state the overlay renders and the source status it follows. */
export function publishDevState(params: {
  state: SessionState;
  source: SourceStatus | null;
}): void {
  devState.state = params.state;
  devState.source = params.source;
}
