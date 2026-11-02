import { describe, expect, it } from 'vitest';
import { checkTestBeat, formatReadout } from './overlay.mjs';

const overlay = {
  isCollapsed: false,
  status: 'Listening',
  key: 'A minor',
  relative: 'C major',
  tuning: null,
  bpm: '120',
  alternate: null,
  notes: null,
  history: [],
};

describe('formatReadout', () => {
  it('says when the overlay is missing', () => {
    expect(formatReadout(null)).toBe('overlay not mounted');
  });

  it('shows tuning and the alternate tempo when present', () => {
    const readout = formatReadout({ ...overlay, tuning: '+12c', alternate: '60', history: ['x'] });
    expect(readout).toBe('Listening | key A minor +12c | bpm 120 (60) | history 1');
  });

  it('dashes readouts that are still empty', () => {
    expect(formatReadout({ ...overlay, key: null, bpm: null })).toBe(
      'Listening | key — | bpm — | history 0',
    );
  });
});

describe('checkTestBeat', () => {
  it('passes A minor or C major near 120 BPM', () => {
    expect(checkTestBeat({ overlay, errors: [] })).toEqual([]);
    expect(
      checkTestBeat({ overlay: { ...overlay, key: 'C major', bpm: '123' }, errors: [] }),
    ).toEqual([]);
  });

  it('flags a missing overlay', () => {
    expect(checkTestBeat({ overlay: null, errors: [] })).toEqual(['the overlay never mounted']);
  });

  it('flags a wrong key and a tempo outside the tolerance', () => {
    const problems = checkTestBeat({
      overlay: { ...overlay, key: 'E minor', bpm: '90' },
      errors: [],
    });
    expect(problems).toEqual([
      'key is "E minor", expected A minor or C major',
      'bpm is "90", expected 120',
    ]);
  });

  it('fails on console errors', () => {
    expect(checkTestBeat({ overlay, errors: ['page: boom'] })).toEqual([
      'console error: page: boom',
    ]);
  });
});
