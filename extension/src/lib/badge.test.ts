import { describe, expect, it } from 'vitest';
import { createSessionState, type Estimate, type SessionState } from '@hunch/ui';
import { badgeText } from './badge';

const TAB_ID = 3;

function estimate(bpm: number | null): Estimate {
  return {
    key: 'A minor',
    keyConfidence: 0.6,
    bpm,
    bpmConfidence: 0.7,
    bpmAlternate: null,
    tuningCents: null,
    secondsAnalyzed: 8,
  };
}

function withWindowEstimate(bpm: number | null): SessionState {
  return {
    ...createSessionState(TAB_ID),
    window: { windowId: 1, startedAt: 0, recentEstimates: [estimate(bpm)], hasEnded: false },
  };
}

describe('badgeText', () => {
  it('shows ON while listening without an estimate', () => {
    expect(badgeText(createSessionState(TAB_ID))).toBe('ON');
  });

  it('shows ON when the estimate has no bpm yet', () => {
    expect(badgeText(withWindowEstimate(null))).toBe('ON');
  });

  it('shows the rounded bpm of the latest estimate', () => {
    expect(badgeText(withWindowEstimate(141.6))).toBe('142');
  });

  it('falls back to the last ended window when no window is open', () => {
    const state: SessionState = {
      ...createSessionState(TAB_ID),
      history: [{ windowId: 1, endedAt: 0, durationSeconds: 20, estimate: estimate(90.2) }],
    };

    expect(badgeText(state)).toBe('90');
  });

  it('shows ! on error, over any estimate', () => {
    expect(badgeText({ ...withWindowEstimate(120), error: 'capture refused' })).toBe('!');
  });
});
