// Dev-only helpers for the browser mixins that check and debug the frontends: the desktop app's
// dev state and summaries of session data. Production code never imports this at runtime.
// Node runs this file directly in the mixins, so it imports nothing but types.
import type { Estimate, SessionEvent } from './estimate';
import type { SessionState, WindowState } from './session-state';
import type { SourceStatus } from './sources';

/** The global a dev build of the desktop app publishes its `DesktopDevState` on. */
export const DESKTOP_DEV_STATE_GLOBAL = '__hunchDev';
/** Entries kept in `DesktopDevState.log`. */
export const DEV_LOG_LIMIT = 200;

/** One message from the backend. */
export type DevLogMessage =
  { kind: 'sessionEvent'; event: SessionEvent } | { kind: 'sourceStatus'; status: SourceStatus };

/** A message numbered in arrival order, so readers can tell what is new and what they missed. */
export type DevLogEntry = DevLogMessage & { seq: number; receivedAt: number };

/** What the desktop app's UI holds, published on `window.__hunchDev` in dev builds only. */
export interface DesktopDevState {
  /** The session state the overlay renders. */
  state: SessionState;
  /** The latest `source-status`, `null` until one arrives or after switching sources. */
  source: SourceStatus | null;
  /** The last `DEV_LOG_LIMIT` raw `session-event`s and `source-status`es, oldest first. */
  log: DevLogEntry[];
}

const EMPTY = '—';

function formatBpm(bpm: number | null): string {
  return bpm === null ? EMPTY : bpm.toFixed(1);
}

/** The newest estimate, from the current window or else the latest finished one. */
export function newestEstimate(state: SessionState | null): Estimate | null {
  return state?.window?.recentEstimates.at(-1) ?? state?.history[0]?.estimate ?? null;
}

function describeWindow(window: WindowState | null): string {
  if (!window) return 'no window';
  const estimates = `${String(window.recentEstimates.length)} estimates`;
  return `window ${String(window.windowId)}${window.hasEnded ? ' ended' : ''} (${estimates})`;
}

/** One line for the live readout. */
export function formatSession(state: SessionState | null): string {
  if (!state) return 'no session state';
  const estimate = newestEstimate(state);
  const parts = [
    describeWindow(state.window),
    `key ${estimate?.key ?? EMPTY}`,
    `bpm ${formatBpm(estimate?.bpm ?? null)}`,
    `history ${String(state.history.length)}`,
  ];
  if (state.isStopped) parts.push('stopped');
  if (state.error) parts.push(`error ${state.error}`);
  return parts.join(' | ');
}

function formatSessionEvent(event: SessionEvent): string {
  switch (event.type) {
    case 'windowStarted':
      return `window ${String(event.windowId)} started at ${event.atSeconds.toFixed(1)}s of audio`;
    case 'estimate':
      return `window ${String(event.windowId)} estimate: key ${event.estimate.key ?? EMPTY}, bpm ${formatBpm(event.estimate.bpm)}`;
    case 'windowEnded': {
      const length = `${event.durationSeconds.toFixed(1)}s${event.tooShort ? ', too short' : ''}`;
      const key = event.estimate?.key ?? EMPTY;
      const bpm = formatBpm(event.estimate?.bpm ?? null);
      return `window ${String(event.windowId)} ended after ${length}: key ${key}, bpm ${bpm}`;
    }
  }
}

function formatSourceStatus(status: SourceStatus): string {
  const message = status.message ? `: ${status.message}` : '';
  return `source ${status.sourceId ?? 'none'} ${status.state}${message}`;
}

/** One line for a log entry. */
export function formatLogEntry(entry: DevLogEntry): string {
  return entry.kind === 'sessionEvent'
    ? formatSessionEvent(entry.event)
    : formatSourceStatus(entry.status);
}
