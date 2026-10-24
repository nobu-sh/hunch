import type { Estimate, SessionEvent } from './messages';

export const HISTORY_LIMIT = 10;
/** Estimates kept per window; enough for the lock rule's streak. */
export const RECENT_ESTIMATES_LIMIT = 3;

/** The window being listened to, or the last one that ended (`hasEnded`). */
export interface WindowState {
  windowId: number;
  /** Wall-clock ms when the background saw the window start. */
  startedAt: number;
  /** Newest last. */
  recentEstimates: Estimate[];
  hasEnded: boolean;
}

export interface HistoryEntry {
  windowId: number;
  /** Wall-clock ms when the background saw the window end. */
  endedAt: number;
  durationSeconds: number;
  estimate: Estimate | null;
}

/** Everything the overlay and badge need for one enabled tab. Lives in `storage.session`. */
export interface TabState {
  tabId: number;
  window: WindowState | null;
  /** Newest first. Windows that were `tooShort` are left out. */
  history: HistoryEntry[];
  error: string | null;
}

export function createTabState(tabId: number): TabState {
  return { tabId, window: null, history: [], error: null };
}

export function withError(params: { state: TabState; error: string }): TabState {
  return { ...params.state, error: params.error };
}

export function latestEstimate(window: WindowState | null): Estimate | null {
  return window?.recentEstimates.at(-1) ?? null;
}

function appendEstimate(params: { window: WindowState; estimate: Estimate }): WindowState {
  const recentEstimates = [...params.window.recentEstimates, params.estimate].slice(
    -RECENT_ESTIMATES_LIMIT,
  );
  return { ...params.window, recentEstimates };
}

function endWindow(params: {
  state: TabState;
  event: Extract<SessionEvent, { type: 'windowEnded' }>;
  now: number;
}): TabState {
  const { state, event, now } = params;
  // Short windows are UI sounds and notifications: drop them without a trace.
  if (event.tooShort) return { ...state, window: null };

  const entry: HistoryEntry = {
    windowId: event.windowId,
    endedAt: now,
    durationSeconds: event.durationSeconds,
    estimate: event.estimate,
  };
  const history = [entry, ...state.history].slice(0, HISTORY_LIMIT);
  const current = state.window?.windowId === event.windowId ? state.window : null;
  if (current === null) return { ...state, history };

  const ended = { ...current, hasEnded: true };
  const window = event.estimate
    ? appendEstimate({ window: ended, estimate: event.estimate })
    : ended;
  return { ...state, window, history };
}

/** Folds one `SessionEvent` into a tab's state. `now` stamps window start and end times. */
export function applySessionEvent(params: {
  state: TabState;
  event: SessionEvent;
  now: number;
}): TabState {
  const { state, event, now } = params;
  switch (event.type) {
    case 'windowStarted':
      return {
        ...state,
        window: { windowId: event.windowId, startedAt: now, recentEstimates: [], hasEnded: false },
      };
    case 'estimate': {
      const isCurrentWindow = state.window?.windowId === event.windowId;
      if (!state.window || !isCurrentWindow) return state;
      return {
        ...state,
        window: appendEstimate({ window: state.window, estimate: event.estimate }),
      };
    }
    case 'windowEnded':
      return endWindow({ state, event, now });
  }
}
