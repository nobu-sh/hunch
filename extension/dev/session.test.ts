import type { Estimate, SessionState, WindowState } from '@hunch/ui';
import { describe, expect, it } from 'vitest';
import { checkTestBeatSession } from './session.ts';

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
