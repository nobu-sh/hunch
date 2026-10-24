import { describe, expect, it } from 'vitest';
import { isBpmLocked, isKeyLocked } from './lock';
import type { Estimate } from './messages';

function estimate(overrides: Partial<Estimate> = {}): Estimate {
  return {
    key: 'F# major',
    keyConfidence: 0.7,
    bpm: 142,
    bpmConfidence: 0.8,
    bpmAlternate: null,
    tuningCents: null,
    secondsAnalyzed: 6,
    ...overrides,
  };
}

describe('isBpmLocked', () => {
  it('locks after three confident estimates with the same rounded bpm', () => {
    const estimates = [141.8, 142.1, 142.3].map((bpm) => estimate({ bpm }));

    expect(isBpmLocked(estimates)).toBe(true);
  });

  it('needs three estimates', () => {
    expect(isBpmLocked([estimate(), estimate()])).toBe(false);
  });

  it('does not lock while the rounded bpm changes', () => {
    const estimates = [142, 142, 143].map((bpm) => estimate({ bpm }));

    expect(isBpmLocked(estimates)).toBe(false);
  });

  it('does not lock below the confidence threshold', () => {
    const estimates = [estimate(), estimate(), estimate({ bpmConfidence: 0.59 })];

    expect(isBpmLocked(estimates)).toBe(false);
  });

  it('does not lock without a bpm', () => {
    const estimates = [estimate({ bpm: null }), estimate({ bpm: null }), estimate({ bpm: null })];

    expect(isBpmLocked(estimates)).toBe(false);
  });

  it('only looks at the latest streak', () => {
    const estimates = [100, 142, 142, 142].map((bpm) => estimate({ bpm }));

    expect(isBpmLocked(estimates)).toBe(true);
  });
});

describe('isKeyLocked', () => {
  it('locks after three confident estimates with the same key', () => {
    expect(isKeyLocked([estimate(), estimate(), estimate({ keyConfidence: 0.55 })])).toBe(true);
  });

  it('does not lock while the scale changes', () => {
    const estimates = [estimate(), estimate({ key: 'C# major' }), estimate()];

    expect(isKeyLocked(estimates)).toBe(false);
  });

  it('keeps locking when the key flips to its relative', () => {
    const estimates = [estimate(), estimate({ key: 'D# minor' }), estimate()];

    expect(isKeyLocked(estimates)).toBe(true);
  });

  it('does not lock below the confidence threshold', () => {
    expect(isKeyLocked([estimate(), estimate(), estimate({ keyConfidence: 0.5 })])).toBe(false);
  });

  it('does not lock without a key', () => {
    const estimates = [estimate({ key: null }), estimate({ key: null }), estimate({ key: null })];

    expect(isKeyLocked(estimates)).toBe(false);
  });
});
