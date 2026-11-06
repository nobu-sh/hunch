import type { Estimate, SessionEvent, SessionState, WindowState } from '@hunch/ui';
import { describe, expect, it } from 'vitest';
import {
  checkTestBeatSession,
  checkTestMixSession,
  formatSession,
  formatWindowEvent,
  latestEstimate,
} from './session.ts';

const estimate: Estimate = {
  key: 'A minor',
  keyConfidence: 0.8,
  bpm: 120.2,
  bpmConfidence: 0.9,
  bpmAlternate: null,
  tuningCents: 0,
  secondsAnalyzed: 4,
};

const currentWindow: WindowState = {
  windowId: 2,
  startedAt: 0,
  recentEstimates: [estimate],
  hasEnded: false,
};

const state: SessionState = {
  tabId: 7,
  window: currentWindow,
  history: [],
  error: null,
};

const empty: SessionState = { tabId: 7, window: null, history: [], error: null };

describe('latestEstimate', () => {
  it('prefers the current window', () => {
    expect(latestEstimate(state)).toBe(estimate);
  });

  it('falls back to the newest finished window', () => {
    const finished = { ...estimate, bpm: 60 };
    const history = [{ windowId: 1, endedAt: 0, durationSeconds: 5, estimate: finished }];
    expect(latestEstimate({ ...empty, history })).toBe(finished);
  });

  it('is null without any estimate', () => {
    expect(latestEstimate(empty)).toBeNull();
    expect(latestEstimate(null)).toBeNull();
  });
});

describe('formatSession', () => {
  it('summarizes the window and the latest estimate', () => {
    expect(formatSession(state)).toBe(
      'window 2 (1 estimates) | key A minor | bpm 120.2 | history 0',
    );
  });

  it('dashes missing values and shows errors', () => {
    expect(formatSession({ ...empty, error: 'capture ended' })).toBe(
      'no window | key — | bpm — | history 0 | error capture ended',
    );
  });

  it('says when there is no state', () => {
    expect(formatSession(null)).toBe('no session state');
  });
});

describe('checkTestBeatSession', () => {
  it('passes a window with estimates near the test tempo', () => {
    expect(checkTestBeatSession({ state, errors: [] })).toEqual([]);
  });

  it('accepts half and double time and ignores the key', () => {
    const halfTime = { ...estimate, key: 'D minor', bpm: 60.4 };
    const window = { ...currentWindow, recentEstimates: [halfTime] };
    expect(checkTestBeatSession({ state: { ...state, window }, errors: [] })).toEqual([]);
  });

  it('fails without session state', () => {
    expect(checkTestBeatSession({ state: null, errors: [] })).toEqual([
      'the extension has no session state for the tab',
    ]);
  });

  it('fails when no window has estimates', () => {
    expect(checkTestBeatSession({ state: empty, errors: [] })).toEqual([
      'no window has estimates: audio never reached the analyzer',
    ]);
  });

  it('fails on a tempo far from the test beat', () => {
    const window = { ...currentWindow, recentEstimates: [{ ...estimate, bpm: 90 }] };
    expect(checkTestBeatSession({ state: { ...state, window }, errors: [] })).toEqual([
      'bpm is 90.0, nowhere near 120',
    ]);
  });

  it('fails on session and console errors', () => {
    expect(
      checkTestBeatSession({ state: { ...state, error: 'boom' }, errors: ['page: x'] }),
    ).toEqual(['the session reported an error: boom', 'console error: page: x']);
  });
});

describe('checkTestMixSession', () => {
  const oldEstimate = { ...estimate, bpm: 120 };
  const newEstimate = { ...estimate, key: 'F# minor', bpm: 140 };
  const split: SessionEvent[] = [
    {
      type: 'windowEnded',
      windowId: 1,
      estimate: oldEstimate,
      durationSeconds: 40,
      tooShort: false,
      reason: 'change',
    },
    { type: 'windowStarted', windowId: 2, atSeconds: 40, reason: 'change' },
  ];
  const splitState: SessionState = {
    ...state,
    window: { ...currentWindow, recentEstimates: [newEstimate] },
    history: [{ windowId: 1, endedAt: 0, durationSeconds: 40, estimate: oldEstimate }],
  };

  it('passes a change from the old tempo to the new', () => {
    expect(checkTestMixSession({ state: splitState, windowEvents: split, errors: [] })).toEqual([]);
  });

  it('fails without a window that ended on a change', () => {
    const windowEvents = split.map((event) =>
      event.type === 'windowEnded' ? { ...event, reason: 'silence' as const } : event,
    );
    expect(checkTestMixSession({ state: splitState, windowEvents, errors: [] })).toEqual([
      'no window ended on a tempo change',
    ]);
  });

  it('fails when the new window still reads the old tempo', () => {
    const window = { ...currentWindow, recentEstimates: [oldEstimate] };
    expect(
      checkTestMixSession({ state: { ...splitState, window }, windowEvents: split, errors: [] }),
    ).toEqual(['the new window has bpm 120.0, not near 140']);
  });

  it('fails when the old window left no history row', () => {
    expect(
      checkTestMixSession({
        state: { ...splitState, history: [] },
        windowEvents: split,
        errors: [],
      }),
    ).toEqual(['window 1 has no history row']);
  });
});

describe('formatWindowEvent', () => {
  it('says why a window started and ended', () => {
    expect(
      formatWindowEvent({ type: 'windowStarted', windowId: 2, atSeconds: 40, reason: 'change' }),
    ).toBe('window 2 started at 40.0 s on change');
    expect(
      formatWindowEvent({
        type: 'windowEnded',
        windowId: 1,
        estimate,
        durationSeconds: 40,
        tooShort: false,
        reason: 'change',
      }),
    ).toBe('window 1 ended on change after 40.0 s: key A minor, bpm 120.2');
  });
});
