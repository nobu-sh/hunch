import { describe, expect, it } from 'vitest';
import { isBpmLocked, isKeyLocked } from './lock';
import type { Estimate } from './estimate';

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
  it('locks after two confident estimates with about the same bpm', () => {
    const estimates = [141.2, 143.4].map((bpm) => estimate({ bpm }));

    expect(isBpmLocked(estimates)).toBe(true);
  });

  it('needs two estimates', () => {
    expect(isBpmLocked([estimate()])).toBe(false);
  });

  it('does not lock while the bpm moves more than 2%', () => {
    const estimates = [138, 142].map((bpm) => estimate({ bpm }));

    expect(isBpmLocked(estimates)).toBe(false);
  });

  it('does not lock below the confidence threshold', () => {
    const estimates = [estimate(), estimate({ bpmConfidence: 0.64 })];

    expect(isBpmLocked(estimates)).toBe(false);
  });

  it('does not lock without a bpm', () => {
    const estimates = [estimate({ bpm: null }), estimate({ bpm: null })];

    expect(isBpmLocked(estimates)).toBe(false);
  });

  it('does not lock right after the bpm appears', () => {
    expect(isBpmLocked([estimate({ bpm: null }), estimate()])).toBe(false);
  });

  it('only looks at the latest streak', () => {
    const estimates = [100, 142, 142].map((bpm) => estimate({ bpm }));

    expect(isBpmLocked(estimates)).toBe(true);
  });
});

describe('isKeyLocked', () => {
  it('locks after three confident estimates with the same key', () => {
    expect(isKeyLocked([estimate(), estimate(), estimate({ keyConfidence: 0.6 })])).toBe(true);
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
    expect(isKeyLocked([estimate(), estimate(), estimate({ keyConfidence: 0.59 })])).toBe(false);
  });

  it('needs three estimates', () => {
    expect(isKeyLocked([estimate(), estimate()])).toBe(false);
  });

  it('does not lock without a key', () => {
    const estimates = [estimate({ key: null }), estimate({ key: null }), estimate({ key: null })];

    expect(isKeyLocked(estimates)).toBe(false);
  });
});
