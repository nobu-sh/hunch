// Summarizing and checking the background's session state for a tab, as `readTabSession`
// returns it.
import type { Estimate, SessionState, WindowState } from '@hunch/ui';
import { TEST_BEAT } from './test-beat.ts';

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

function isNearTestBeatTempo(bpm: number): boolean {
  return BPM_MULTIPLES.some((multiple) => {
    const expected = TEST_BEAT.bpm * multiple;
    return Math.abs(bpm - expected) <= expected * BPM_TOLERANCE;
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
  if (bpm !== null && !isNearTestBeatTempo(bpm)) {
    problems.push(`bpm is ${bpm.toFixed(1)}, nowhere near ${String(TEST_BEAT.bpm)}`);
  }
  for (const error of errors) problems.push(`console error: ${error}`);
  return problems;
}
