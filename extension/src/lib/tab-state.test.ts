import { describe, expect, it } from 'vitest';
import type { Estimate, SessionEvent } from './messages';
import {
  HISTORY_LIMIT,
  RECENT_ESTIMATES_LIMIT,
  applySessionEvent,
  createTabState,
  latestEstimate,
  type TabState,
} from './tab-state';

const TAB_ID = 7;
const NOW = 1_000_000;

function estimate(overrides: Partial<Estimate> = {}): Estimate {
  return {
    key: 'F# major',
    keyConfidence: 0.7,
    bpm: 142,
    bpmConfidence: 0.8,
    bpmAlternate: null,
    tuningCents: 3,
    secondsAnalyzed: 6,
    ...overrides,
  };
}

function apply(state: TabState, ...events: SessionEvent[]): TabState {
  return events.reduce((next, event) => applySessionEvent({ state: next, event, now: NOW }), state);
}

const started = (windowId: number): SessionEvent => ({
  type: 'windowStarted',
  windowId,
  atSeconds: 0,
});

const ended = (
  windowId: number,
  overrides: Partial<Extract<SessionEvent, { type: 'windowEnded' }>> = {},
): SessionEvent => ({
  type: 'windowEnded',
  windowId,
  estimate: estimate(),
  durationSeconds: 30,
  tooShort: false,
  ...overrides,
});

describe('applySessionEvent', () => {
  it('opens a window stamped with the current time', () => {
    const state = apply(createTabState(TAB_ID), started(1));

    expect(state.window).toEqual({
      windowId: 1,
      startedAt: NOW,
      recentEstimates: [],
      hasEnded: false,
    });
  });

  it('keeps only the most recent estimates of the open window', () => {
    const estimates = [140, 141, 142, 143].map((bpm) => estimate({ bpm }));
    const events = estimates.map((value): SessionEvent => ({
      type: 'estimate',
      windowId: 1,
      estimate: value,
    }));

    const state = apply(createTabState(TAB_ID), started(1), ...events);

    expect(state.window?.recentEstimates).toHaveLength(RECENT_ESTIMATES_LIMIT);
    expect(latestEstimate(state.window)?.bpm).toBe(143);
  });

  it('ignores estimates for a window that is not current', () => {
    const state = apply(createTabState(TAB_ID), started(2), {
      type: 'estimate',
      windowId: 1,
      estimate: estimate(),
    });

    expect(state.window?.recentEstimates).toEqual([]);
  });

  it('marks an ended window, keeps its final estimate, and records it in history', () => {
    const final = estimate({ bpm: 150 });

    const state = apply(createTabState(TAB_ID), started(1), ended(1, { estimate: final }));

    expect(state.window?.hasEnded).toBe(true);
    expect(latestEstimate(state.window)).toEqual(final);
    expect(state.history).toEqual([
      { windowId: 1, endedAt: NOW, durationSeconds: 30, estimate: final },
    ]);
  });

  it('drops a window that was too short, without a history entry', () => {
    const state = apply(createTabState(TAB_ID), started(1), ended(1, { tooShort: true }));

    expect(state.window).toBeNull();
    expect(state.history).toEqual([]);
  });

  it('records a window that ended without any estimate', () => {
    const state = apply(createTabState(TAB_ID), started(1), ended(1, { estimate: null }));

    expect(state.window?.recentEstimates).toEqual([]);
    expect(state.history[0]?.estimate).toBeNull();
  });

  it('keeps history newest first and bounded', () => {
    const events = Array.from({ length: HISTORY_LIMIT + 2 }, (_, index) => [
      started(index + 1),
      ended(index + 1),
    ]).flat();

    const state = apply(createTabState(TAB_ID), ...events);

    expect(state.history).toHaveLength(HISTORY_LIMIT);
    expect(state.history[0]?.windowId).toBe(HISTORY_LIMIT + 2);
  });

  it('replaces an ended window when the next one starts', () => {
    const state = apply(createTabState(TAB_ID), started(1), ended(1), started(2));

    expect(state.window?.windowId).toBe(2);
    expect(state.window?.hasEnded).toBe(false);
    expect(state.history).toHaveLength(1);
  });
});
