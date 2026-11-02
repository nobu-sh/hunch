import type { OverlayView } from '../overlay-view';
import type { AudioSource, SourceList, SourceStatus } from '../sources';

/** Overlay views and sources for component tests and the preview page. */

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

export const SPEAKERS_SOURCE: AudioSource = {
  id: 'device:speakers',
  kind: 'device',
  name: 'Speakers (Realtek Audio)',
  isDefault: true,
  isOutput: true,
};

export const HEADSET_SOURCE: AudioSource = {
  id: 'device:headset',
  kind: 'device',
  name: 'Headset Earphone',
  isDefault: false,
  isOutput: true,
};

export const MIC_SOURCE: AudioSource = {
  id: 'device:mic',
  kind: 'device',
  name: 'Microphone Array',
  isDefault: true,
  isOutput: false,
};

export const SPOTIFY_SOURCE: AudioSource = {
  id: 'app:spotify',
  kind: 'app',
  name: 'Spotify',
  isDefault: false,
  isOutput: true,
};

export const BROWSER_SOURCE: AudioSource = {
  id: 'app:chrome',
  kind: 'app',
  name: 'Google Chrome',
  isDefault: false,
  isOutput: true,
};

export const SOURCE_LIST: SourceList = {
  sources: [SPOTIFY_SOURCE, BROWSER_SOURCE, SPEAKERS_SOURCE, HEADSET_SOURCE, MIC_SOURCE],
  listsApps: true,
};

/** A platform that can only capture whole devices. */
export const DEVICES_ONLY_LIST: SourceList = {
  sources: [SPEAKERS_SOURCE, HEADSET_SOURCE, MIC_SOURCE],
  listsApps: false,
};

export const ENDED_STATUS: SourceStatus = {
  sourceId: SPOTIFY_SOURCE.id,
  state: 'ended',
  message: null,
};

export const SILENT_STATUS: SourceStatus = {
  sourceId: BROWSER_SOURCE.id,
  state: 'silent',
  message: null,
};

export const FAILED_STATUS: SourceStatus = {
  sourceId: HEADSET_SOURCE.id,
  state: 'failed',
  message: 'The device is in use by another app',
};
