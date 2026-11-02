/** Mirrors the `Estimate` object returned by `Analyzer.push` and `Session.push` in crates/hunch-wasm. */
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

/** Mirrors `SessionEvent` from crates/hunch-wasm. Timings are seconds of audio, not wall clock. */
export type SessionEvent =
  | { type: 'windowStarted'; windowId: number; atSeconds: number }
  | { type: 'estimate'; windowId: number; estimate: Estimate }
  | {
      type: 'windowEnded';
      windowId: number;
      estimate: Estimate | null;
      durationSeconds: number;
      tooShort: boolean;
    };
