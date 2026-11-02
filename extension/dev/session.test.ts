import type { Estimate, SessionState, WindowState } from '@hunch/ui';
import { describe, expect, it } from 'vitest';
import { checkTestBeatSession, formatSession, latestEstimate } from './session.ts';

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
