// What a frontend can listen to, and how a source's status reads to the user. Mirrors the desktop
// backend's `list_sources` and `source-status` JSON.

export type SourceKind = 'device' | 'app';

export interface AudioSource {
  id: string;
  kind: SourceKind;
  name: string;
  isDefault: boolean;
  /** For devices: an output (what plays) rather than an input (a microphone or line in). */
  isOutput: boolean;
}

export interface SourceList {
  sources: AudioSource[];
  /** False where single apps can't be captured, so only devices are offered. */
  listsApps: boolean;
}

export type SourceState = 'listening' | 'ended' | 'silent' | 'failed';

export interface SourceStatus {
  sourceId: string | null;
  state: SourceState;
  message: string | null;
}

export type NoticeTone = 'info' | 'warning' | 'error';

export interface SourceNotice {
  tone: NoticeTone;
  message: string;
  /** The backend's own words, when they add something. */
  detail: string | null;
}

export const SILENT_SOURCE_MESSAGE = 'No sound from this source. It may be protected audio.';

/** The labels shown after a source's name in a list, such as `default` and `output`. */
export function sourceBadges(source: AudioSource): string[] {
  const badges: string[] = [];
  if (source.isDefault) badges.push('default');
  if (source.kind === 'device') badges.push(source.isOutput ? 'output' : 'input');
  return badges;
}

function endedMessage(source: AudioSource | null): string {
  if (source === null) return 'The source stopped';
  return source.kind === 'app' ? `${source.name} closed` : `${source.name} disconnected`;
}

/**
 * A plain message for a source problem, or null while it's listening. `source` is the source the
 * status is about, when it's known.
 */
export function sourceNotice(params: {
  status: SourceStatus;
  source: AudioSource | null;
}): SourceNotice | null {
  const { status, source } = params;
  switch (status.state) {
    case 'listening':
      return null;
    case 'ended':
      return { tone: 'warning', message: endedMessage(source), detail: null };
    case 'silent':
      return { tone: 'warning', message: SILENT_SOURCE_MESSAGE, detail: null };
    case 'failed': {
      const message = source
        ? `Couldn't listen to ${source.name}`
        : "Couldn't listen to the source";
      return { tone: 'error', message, detail: status.message };
    }
  }
}

/** A rejection's message: Tauri commands reject with a plain string. */
export function errorMessage(error: unknown): string {
  if (typeof error === 'string') return error;
  if (error instanceof Error) return error.message;
  return String(error);
}
