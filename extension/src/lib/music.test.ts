import { describe, expect, it } from 'vitest';
import { parseKey, relativeKey, scaleNotes, shortKeyLabel, type Key } from './music';

const F_SHARP_MAJOR: Key = { tonic: 6, mode: 'major' };
const D_SHARP_MINOR: Key = { tonic: 3, mode: 'minor' };

describe('parseKey', () => {
  it('parses major and minor labels', () => {
    expect(parseKey('F# major')).toEqual(F_SHARP_MAJOR);
    expect(parseKey('D# minor')).toEqual(D_SHARP_MINOR);
  });

  it('rejects labels it does not understand', () => {
    expect(parseKey('H major')).toBeNull();
    expect(parseKey('C dorian')).toBeNull();
    expect(parseKey('')).toBeNull();
  });
});

describe('relativeKey', () => {
  it('maps a major key to its relative minor and back', () => {
    expect(relativeKey(F_SHARP_MAJOR)).toEqual(D_SHARP_MINOR);
    expect(relativeKey(D_SHARP_MINOR)).toEqual(F_SHARP_MAJOR);
  });

  it('wraps around the octave', () => {
    expect(relativeKey({ tonic: 9, mode: 'minor' })).toEqual({ tonic: 0, mode: 'major' });
  });
});

describe('shortKeyLabel', () => {
  it('marks minor keys with m', () => {
    expect(shortKeyLabel(F_SHARP_MAJOR)).toBe('F#');
    expect(shortKeyLabel(D_SHARP_MINOR)).toBe('D#m');
  });
});

describe('scaleNotes', () => {
  it('spells a major scale in sharps from the tonic', () => {
    expect(scaleNotes(F_SHARP_MAJOR)).toEqual(['F#', 'G#', 'A#', 'B', 'C#', 'D#', 'F']);
  });

  it('spells the natural minor scale, sharing the notes of its relative major', () => {
    expect(scaleNotes(D_SHARP_MINOR)).toEqual(['D#', 'F', 'F#', 'G#', 'A#', 'B', 'C#']);
  });
});
