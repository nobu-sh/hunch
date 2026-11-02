import type { OverlayView } from '../overlay-view';

/** Overlay views for component tests and the preview page. */

export const LISTENING_VIEW: OverlayView = {
  status: 'listening…',
  statusTone: 'idle',
  key: null,
  bpm: null,
  tuning: null,
  pill: '— · —',
  history: [],
};

export const ANALYZING_VIEW: OverlayView = {
  status: 'analyzing 6s',
  statusTone: 'active',
  key: {
    label: 'A minor',
    relative: 'C',
    notes: 'A B C D E F G',
    confidence: 0.42,
    isLocked: false,
  },
  bpm: { label: '120', alternate: '×2 240', confidence: 0.618, isLocked: false },
  tuning: '+23¢',
  pill: '120 · Am',
  history: [],
};

export const LOCKED_VIEW: OverlayView = {
  status: 'locked',
  statusTone: 'locked',
  key: {
    label: 'F# major',
    relative: 'D#m',
    notes: 'F# G# A# B C# D# E#',
    confidence: 0.91,
    isLocked: true,
  },
  bpm: { label: '142', alternate: '½ 71', confidence: 0.87, isLocked: true },
  tuning: null,
  pill: '142 · F#',
  history: [
    { time: '21:04', key: 'A minor', bpm: '120', copyText: 'A minor · 120 BPM' },
    { time: '20:58', key: '—', bpm: '96', copyText: '96 BPM' },
  ],
};

export const ERROR_VIEW: OverlayView = {
  ...LISTENING_VIEW,
  status: 'Tab capture failed',
  statusTone: 'error',
};
