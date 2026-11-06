// Summarizing and checking the background's session state for a tab, as `readTabSession`
// returns it.
import type { Estimate, SessionEvent, SessionState, WindowState } from '@hunch/ui';
import { TEST_BEAT, TEST_MIX } from './test-beat.ts';

/** Loose on purpose: this checks that audio gets analyzed, not how accurately. */
const BPM_TOLERANCE = 0.1;
/** Tempo trackers often land on half or double time; both count. */
const BPM_MULTIPLES = [0.5, 1, 2];
const EMPTY = '—';

/** The newest estimate, from the current window or else the latest finished one. */
export function latestEstimate(state: SessionState | null): Estimate | null {
  return state?.window?.recentEstimates.at(-1) ?? state?.history[0]?.estimate ?? null;
}

function hasEstimates(state: SessionState): boolean {
  const isWindowEstimated = (state.window?.recentEstimates.length ?? 0) > 0;
  const isHistoryEstimated = state.history.some((entry) => entry.estimate !== null);
  return isWindowEstimated || isHistoryEstimated;
}

function describeWindow(window: WindowState | null): string {
  if (!window) return 'no window';
  const estimates = `${String(window.recentEstimates.length)} estimates`;
  return `window ${String(window.windowId)}${window.hasEnded ? ' ended' : ''} (${estimates})`;
}

/** One line for the live readout. */
export function formatSession(state: SessionState | null): string {
  if (!state) return 'no session state';
  const estimate = latestEstimate(state);
  const bpm = estimate?.bpm ?? null;
  const parts = [
    describeWindow(state.window),
    `key ${estimate?.key ?? EMPTY}`,
    `bpm ${bpm === null ? EMPTY : bpm.toFixed(1)}`,
    `history ${String(state.history.length)}`,
  ];
  if (state.error) parts.push(`error ${state.error}`);
  return parts.join(' | ');
}

function isNearTempo(params: { bpm: number; target: number }): boolean {
  return BPM_MULTIPLES.some((multiple) => {
    const expected = params.target * multiple;
    return Math.abs(params.bpm - expected) <= expected * BPM_TOLERANCE;
  });
}

/**
 * What says the test beat's audio didn't reach the analyzer; empty when it did. Needs a window
 * with estimates and no errors. The key isn't judged, and the tempo only loosely.
 */
export function checkTestBeatSession(params: {
  state: SessionState | null;
  errors: string[];
}): string[] {
  const { state, errors } = params;
  const problems: string[] = [];
  if (!state) problems.push('the extension has no session state for the tab');
  if (state?.error) problems.push(`the session reported an error: ${state.error}`);
  if (state && !hasEstimates(state)) {
    problems.push('no window has estimates: audio never reached the analyzer');
  }
  const bpm = latestEstimate(state)?.bpm ?? null;
  if (bpm !== null && !isNearTempo({ bpm, target: TEST_BEAT.bpm })) {
    problems.push(`bpm is ${bpm.toFixed(1)}, nowhere near ${String(TEST_BEAT.bpm)}`);
  }
  for (const error of errors) problems.push(`console error: ${error}`);
  return problems;
}

const [OLD_SONG, NEW_SONG] = TEST_MIX;

/** One line for a window's start or end, with why it happened. */
export function formatWindowEvent(event: SessionEvent): string {
  const window = `window ${String(event.windowId)}`;
  switch (event.type) {
    case 'windowStarted':
      return `${window} started at ${event.atSeconds.toFixed(1)} s on ${event.reason}`;
    case 'windowEnded': {
      const bpm = event.estimate?.bpm ?? null;
      const result = `key ${event.estimate?.key ?? EMPTY}, bpm ${bpm === null ? EMPTY : bpm.toFixed(1)}`;
      return `${window} ended on ${event.reason} after ${event.durationSeconds.toFixed(1)} s: ${result}`;
    }
    case 'estimate':
      return `${window} estimate`;
  }
}

/** The newest estimate the state holds for window `windowId`: its live readout, or its history row. */
function estimateOfWindow(params: { state: SessionState; windowId: number }): Estimate | null {
  const { state, windowId } = params;
  if (state.window?.windowId === windowId) return latestEstimate(state);
  return state.history.find((entry) => entry.windowId === windowId)?.estimate ?? null;
}

function checkTempo(params: { what: string; estimate: Estimate | null; target: number }): string[] {
  const { what, estimate, target } = params;
  const bpm = estimate?.bpm ?? null;
  if (bpm !== null && isNearTempo({ bpm, target })) return [];
  const read = bpm === null ? 'no bpm' : `bpm ${bpm.toFixed(1)}`;
  return [`${what} has ${read}, not near ${String(target)}`];
}

/**
 * What says the test mix wasn't split from its first song into its second; empty when it was.
 * Needs a window that ended on a change with a history row near the first song's tempo, then a
 * window that started on the change and reads near the second song's.
 */
export function checkTestMixSession(params: {
  state: SessionState | null;
  windowEvents: SessionEvent[];
  errors: string[];
}): string[] {
  const { state, windowEvents, errors } = params;
  const consoleErrors = errors.map((error) => `console error: ${error}`);
  if (!state) return ['the extension has no session state for the tab', ...consoleErrors];

  const ended = windowEvents.find(
    (event) => event.type === 'windowEnded' && event.reason === 'change',
  );
  if (!ended) return ['no window ended on a tempo change', ...consoleErrors];
  const started = windowEvents.find(
    (event) => event.type === 'windowStarted' && event.reason === 'change',
  );
  if (!started) return ['no window started on the tempo change', ...consoleErrors];

  const oldRow = state.history.find((entry) => entry.windowId === ended.windowId);
  const problems = oldRow
    ? checkTempo({ what: 'the old window', estimate: oldRow.estimate, target: OLD_SONG.bpm })
    : [`window ${String(ended.windowId)} has no history row`];
  problems.push(
    ...checkTempo({
      what: 'the new window',
      estimate: estimateOfWindow({ state, windowId: started.windowId }),
      target: NEW_SONG.bpm,
    }),
  );
  if (state.error) problems.push(`the session reported an error: ${state.error}`);
  return [...problems, ...consoleErrors];
}
