const NOTE_NAMES = ['C', 'C#', 'D', 'D#', 'E', 'F', 'F#', 'G', 'G#', 'A', 'A#', 'B'];
const MAJOR_STEPS = [0, 2, 4, 5, 7, 9, 11];
const NATURAL_MINOR_STEPS = [0, 2, 3, 5, 7, 8, 10];
/** Semitones from a major tonic down to its relative minor (F# → D#). */
const RELATIVE_MINOR_OFFSET = 9;
const RELATIVE_MAJOR_OFFSET = 3;
const PITCH_CLASSES = NOTE_NAMES.length;

export type Mode = 'major' | 'minor';

export interface Key {
  /** Pitch class, 0 = C. */
  tonic: number;
  mode: Mode;
}

function noteName(pitchClass: number): string {
  return NOTE_NAMES[pitchClass % PITCH_CLASSES] ?? '?';
}

/** Parses the analyzer's key label, e.g. `F# major` or `D# minor`. */
export function parseKey(label: string): Key | null {
  const [tonicName, mode] = label.split(' ');
  const tonic = NOTE_NAMES.indexOf(tonicName ?? '');
  const isMode = mode === 'major' || mode === 'minor';
  if (tonic === -1 || !isMode) return null;
  return { tonic, mode };
}

/** Compact label: `F#` for major, `D#m` for minor. */
export function shortKeyLabel(key: Key): string {
  return key.mode === 'major' ? noteName(key.tonic) : `${noteName(key.tonic)}m`;
}

export function relativeKey(key: Key): Key {
  return key.mode === 'major'
    ? { tonic: (key.tonic + RELATIVE_MINOR_OFFSET) % PITCH_CLASSES, mode: 'minor' }
    : { tonic: (key.tonic + RELATIVE_MAJOR_OFFSET) % PITCH_CLASSES, mode: 'major' };
}

/** Identifies the note set: relative keys (F# major, D# minor) share one id, the major tonic. */
export function scaleId(key: Key): number {
  return key.mode === 'major' ? key.tonic : relativeKey(key).tonic;
}

/** The seven scale notes from the tonic, in sharps: `F# G# A# B C# D# F`. */
export function scaleNotes(key: Key): string[] {
  const steps = key.mode === 'major' ? MAJOR_STEPS : NATURAL_MINOR_STEPS;
  return steps.map((step) => noteName(key.tonic + step));
}
