/** Mirrors the `Estimate` object returned by `Analyzer.push` in crates/hunch-wasm. */
export interface Estimate {
  key: string | null;
  keyConfidence: number;
  bpm: number | null;
  bpmConfidence: number;
  bpmAlternate: number | null;
  secondsAnalyzed: number;
}

/** Background → offscreen document. `target` keeps other extension pages from answering. */
export interface OffscreenPingRequest {
  target: 'offscreen';
  type: 'ping';
}

/** Offscreen document → analysis Worker. The reply comes back on a transferred MessagePort. */
export interface WorkerPingRequest {
  type: 'ping';
}

/** Analysis Worker → offscreen document → background. */
export interface PingReply {
  version: string;
  estimate: Estimate | undefined;
}

export function isOffscreenPingRequest(message: unknown): message is OffscreenPingRequest {
  if (typeof message !== 'object' || message === null) return false;
  const { target, type } = message as Partial<OffscreenPingRequest>;
  return target === 'offscreen' && type === 'ping';
}
