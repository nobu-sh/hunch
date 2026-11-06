import { describe, expect, it } from 'vitest';
import { formatLogEntry, formatSession, newestEstimate } from './dev';
import type { Estimate } from './estimate';
import type { SessionState } from './session-state';

const estimate: Estimate = {
  key: 'A minor',
  keyConfidence: 0.8,
  bpm: 120.2,
  bpmConfidence: 0.9,
  bpmAlternate: null,
  tuningCents: 0,
  secondsAnalyzed: 4,
};

const state: SessionState = {
  tabId: 7,
  window: { windowId: 2, startedAt: 0, recentEstimates: [estimate], hasEnded: false },
  history: [],
  error: null,
};

const empty: SessionState = { tabId: 7, window: null, history: [], error: null };

describe('newestEstimate', () => {
  it('prefers the current window', () => {
    expect(newestEstimate(state)).toBe(estimate);
  });

  it('falls back to the newest finished window', () => {
    const finished = { ...estimate, bpm: 60 };
    const history = [{ windowId: 1, endedAt: 0, durationSeconds: 5, estimate: finished }];
    expect(newestEstimate({ ...empty, history })).toBe(finished);
  });

  it('is null without any estimate', () => {
    expect(newestEstimate(empty)).toBeNull();
    expect(newestEstimate(null)).toBeNull();
  });
});

describe('formatSession', () => {
  it('summarizes the window and the newest estimate', () => {
    expect(formatSession(state)).toBe(
      'window 2 (1 estimates) | key A minor | bpm 120.2 | history 0',
    );
  });

  it('dashes missing values and shows errors', () => {
    expect(formatSession({ ...empty, error: 'capture ended' })).toBe(
      'no window | key — | bpm — | history 0 | error capture ended',
    );
  });

  it('says when the source stopped', () => {
    expect(formatSession({ ...empty, isStopped: true })).toBe(
      'no window | key — | bpm — | history 0 | stopped',
    );
  });

  it('says when there is no state', () => {
    expect(formatSession(null)).toBe('no session state');
  });
});

describe('formatLogEntry', () => {
  const at = { seq: 1, receivedAt: 0 };

  it('describes a window starting', () => {
    const event = { type: 'windowStarted', windowId: 3, atSeconds: 61.25 } as const;
    expect(formatLogEntry({ ...at, kind: 'sessionEvent', event })).toBe(
      'window 3 started at 61.3s of audio',
    );
  });

  it('describes a window ending with its result', () => {
    const event = {
      type: 'windowEnded',
      windowId: 3,
      estimate,
      durationSeconds: 42,
      tooShort: false,
    } as const;
    expect(formatLogEntry({ ...at, kind: 'sessionEvent', event })).toBe(
      'window 3 ended after 42.0s: key A minor, bpm 120.2',
    );
  });

  it('marks a window that ended too short', () => {
    const event = {
      type: 'windowEnded',
      windowId: 4,
      estimate: null,
      durationSeconds: 0.5,
      tooShort: true,
    } as const;
    expect(formatLogEntry({ ...at, kind: 'sessionEvent', event })).toBe(
      'window 4 ended after 0.5s, too short: key —, bpm —',
    );
  });

  it('describes a source status', () => {
    const status = { sourceId: 'app:spotify.exe', state: 'ended', message: 'closed' } as const;
    expect(formatLogEntry({ ...at, kind: 'sourceStatus', status })).toBe(
      'source app:spotify.exe ended: closed',
    );
  });
});
