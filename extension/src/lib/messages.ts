import type { TabState } from './tab-state';

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

/** Background → offscreen document. `target` keeps other extension pages from answering. */
export interface OffscreenPingRequest {
  target: 'offscreen';
  type: 'ping';
}

/** Background → offscreen document: start capturing a tab with a `tabCapture` stream ID. */
export interface OffscreenStartRequest {
  target: 'offscreen';
  type: 'start';
  tabId: number;
  streamId: string;
}

/** Background → offscreen document: stop capturing a tab and drop its session. */
export interface OffscreenStopRequest {
  target: 'offscreen';
  type: 'stop';
  tabId: number;
}

export type OffscreenRequest = OffscreenPingRequest | OffscreenStartRequest | OffscreenStopRequest;

/** Offscreen document → analysis Worker. The reply comes back on a transferred MessagePort. */
export interface WorkerPingRequest {
  type: 'ping';
}

/** Offscreen document → analysis Worker. Carries the MessagePort the AudioWorklet posts chunks to. */
export interface WorkerStartRequest {
  type: 'start';
  tabId: number;
}

export interface WorkerStopRequest {
  type: 'stop';
  tabId: number;
}

export type WorkerRequest = WorkerPingRequest | WorkerStartRequest | WorkerStopRequest;

/** Analysis Worker → offscreen document → background. */
export interface PingReply {
  version: string;
  /** From the plain `Analyzer`, which may not report tuning. */
  estimate: Omit<Estimate, 'tuningCents'> | undefined;
}

/** Analysis Worker → offscreen document → background, one per `SessionEvent`. */
export interface SessionEventMessage {
  type: 'session-event';
  tabId: number;
  event: SessionEvent;
}

/** Offscreen document → background when a tab's capture fails. */
export interface CaptureErrorMessage {
  type: 'capture-error';
  tabId: number;
  message: string;
}

export type BackgroundMessage = SessionEventMessage | CaptureErrorMessage;

function isObject(message: unknown): message is Record<string, unknown> {
  return typeof message === 'object' && message !== null;
}

export function isOffscreenRequest(message: unknown): message is OffscreenRequest {
  return isObject(message) && message.target === 'offscreen';
}

export function isBackgroundMessage(message: unknown): message is BackgroundMessage {
  if (!isObject(message) || typeof message.tabId !== 'number') return false;
  return message.type === 'session-event' || message.type === 'capture-error';
}

/** Background → overlay content script: the tab's latest state. */
export interface OverlayStateMessage {
  target: 'overlay';
  type: 'state';
  state: TabState;
}

/** Background → overlay content script when Hunch is turned off for the tab. */
export interface OverlayUnmountMessage {
  target: 'overlay';
  type: 'unmount';
}

export type OverlayMessage = OverlayStateMessage | OverlayUnmountMessage;

/** Overlay → background on mount. Replies with the tab's `TabState`, or null when it's off. */
export interface OverlayHelloRequest {
  type: 'overlay-hello';
}

/** Overlay → background: the close button, same as clicking the toolbar icon. */
export interface OverlayCloseRequest {
  type: 'overlay-close';
}

export type OverlayRequest = OverlayHelloRequest | OverlayCloseRequest;

export function isOverlayMessage(message: unknown): message is OverlayMessage {
  return isObject(message) && message.target === 'overlay';
}

export function isOverlayRequest(message: unknown): message is OverlayRequest {
  if (!isObject(message)) return false;
  return message.type === 'overlay-hello' || message.type === 'overlay-close';
}
