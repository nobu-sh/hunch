import { describe, expect, it } from 'vitest';
import type { Estimate } from './estimate';
import { estimateCopyText, formatBpmAlternate, formatTuning, overlayView } from './overlay-view';
import { createSessionState, type SessionState } from './session-state';

const TAB_ID = 4;

function estimate(overrides: Partial<Estimate> = {}): Estimate {
  return {
    key: 'F# major',
    keyConfidence: 0.7,
    bpm: 142,
    bpmConfidence: 0.8,
    bpmAlternate: null,
    tuningCents: 2,
    secondsAnalyzed: 6.4,
    ...overrides,
  };
}

function withWindow(params: { estimates: Estimate[]; hasEnded?: boolean }): SessionState {
  return {
    ...createSessionState(TAB_ID),
    window: {
      windowId: 1,
      startedAt: 0,
      recentEstimates: params.estimates,
      hasEnded: params.hasEnded ?? false,
    },
  };
}

describe('overlayView status', () => {
  it('is listening before any window', () => {
    const view = overlayView(createSessionState(TAB_ID));

    expect(view.status).toBe('listening…');
    expect(view.key).toBeNull();
    expect(view.bpm).toBeNull();
    expect(view.pill).toBe('— · —');
  });

  it('shows the analyzed seconds while not locked', () => {
    const view = overlayView(withWindow({ estimates: [estimate()] }));

    expect(view.status).toBe('analyzing 6s');
    expect(view.statusTone).toBe('active');
    expect(view.key?.isLocked).toBe(false);
  });

  it('shows analyzing without seconds before the first estimate', () => {
    expect(overlayView(withWindow({ estimates: [] })).status).toBe('analyzing');
  });

  it('is locked once key and bpm both lock', () => {
    const view = overlayView(withWindow({ estimates: [estimate(), estimate(), estimate()] }));

    expect(view.status).toBe('locked');
    expect(view.key?.isLocked).toBe(true);
    expect(view.bpm?.isLocked).toBe(true);
  });

  it('can lock on bpm alone when there is no key', () => {
    const noKey = estimate({ key: null });

    expect(overlayView(withWindow({ estimates: [noKey, noKey, noKey] })).status).toBe('locked');
  });

  it('reports silence once the window ended, keeping its values', () => {
    const view = overlayView(withWindow({ estimates: [estimate()], hasEnded: true }));

    expect(view.status).toBe('silence');
    expect(view.bpm?.label).toBe('142');
  });

  it('shows the error over everything else', () => {
    const state = { ...withWindow({ estimates: [estimate()] }), error: 'capture refused' };

    expect(overlayView(state)).toMatchObject({ status: 'capture refused', statusTone: 'error' });
  });
});

describe('overlayView values', () => {
  it('shows the key with its relative key and scale notes', () => {
    const view = overlayView(withWindow({ estimates: [estimate()] }));

    expect(view.key).toMatchObject({
      label: 'F# major',
      relative: 'D#m',
      notes: 'F# G# A# B C# D# F',
    });
  });

  it('rounds the bpm and shows the alternate', () => {
    const view = overlayView(
      withWindow({ estimates: [estimate({ bpm: 149.6, bpmAlternate: 74.8 })] }),
    );

    expect(view.bpm).toMatchObject({ label: '150', alternate: '½ 75' });
  });

  it('builds the collapsed pill from bpm and short key', () => {
    const view = overlayView(withWindow({ estimates: [estimate({ key: 'D# minor' })] }));

    expect(view.pill).toBe('142 · D#m');
  });

  it('lists history rows with copyable text', () => {
    const endedAt = new Date(2026, 9, 23, 9, 5).getTime();
    const state: SessionState = {
      ...createSessionState(TAB_ID),
      history: [{ windowId: 1, endedAt, durationSeconds: 30, estimate: estimate() }],
    };

    expect(overlayView(state).history).toEqual([
      { time: '09:05', key: 'F# major', bpm: '142', copyText: 'F# major · 142 BPM' },
    ]);
  });
});

describe('formatBpmAlternate', () => {
  it('marks a slower alternate as half and a faster one as double', () => {
    expect(formatBpmAlternate({ bpm: 150, alternate: 75 })).toBe('½ 75');
    expect(formatBpmAlternate({ bpm: 150, alternate: 300 })).toBe('×2 300');
  });
});

describe('formatTuning', () => {
  it('hides small offsets', () => {
    expect(formatTuning(14.9)).toBeNull();
    expect(formatTuning(null)).toBeNull();
  });

  it('shows signed cents from 15 up', () => {
    expect(formatTuning(23.2)).toBe('+23¢');
    expect(formatTuning(-15)).toBe('-15¢');
  });
});

describe('estimateCopyText', () => {
  it('leaves out missing values', () => {
    expect(estimateCopyText(estimate({ key: null }))).toBe('142 BPM');
    expect(estimateCopyText(estimate({ bpm: null }))).toBe('F# major');
    expect(estimateCopyText(null)).toBe('—');
  });
});
