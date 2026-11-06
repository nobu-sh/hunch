/** Mirrors `EstimateDto` in crates/hunch-core/src/contract.rs, sent by the WASM bindings and the desktop app. */
export interface Estimate {
  key: string | null;
  keyConfidence: number;
  bpm: number | null;
  bpmConfidence: number;
  bpmAlternate: number | null;
  /** Deviation from A=440 in cents; null when there is no key. */
  tuningCents: number | null;
  secondsAnalyzed: number;
}

/** Why a window started: sound after silence, or a tempo change that went straight into a new song. */
export type WindowStartReason = 'sound' | 'change';

/** Why a window ended: the sound stopped, the tempo changed (the next window starts here), or the stream finished. */
export type WindowEndReason = 'silence' | 'change' | 'finished';

/**
 * Mirrors `SessionEventDto` in crates/hunch-core/src/contract.rs. Timings are seconds of audio, not wall clock.
 * After a change, the new window's estimates for the audio since the change arrive in a burst.
 */
export type SessionEvent =
  | { type: 'windowStarted'; windowId: number; atSeconds: number; reason: WindowStartReason }
  | { type: 'estimate'; windowId: number; estimate: Estimate }
  | {
      type: 'windowEnded';
      windowId: number;
      estimate: Estimate | null;
      durationSeconds: number;
      tooShort: boolean;
      reason: WindowEndReason;
    };
