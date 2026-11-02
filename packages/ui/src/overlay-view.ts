import { isBpmLocked, isKeyLocked } from './lock';
import type { Estimate } from './estimate';
import { parseKey, relativeKey, scaleNotes, shortKeyLabel } from './music';
import { latestEstimate, type HistoryEntry, type SessionState } from './session-state';

/** Tuning offsets smaller than this are normal and not worth showing. */
export const TUNING_DISPLAY_CENTS = 15;
const EMPTY = '—';

export type StatusTone = 'idle' | 'active' | 'locked' | 'error';

export interface KeyView {
  label: string;
  relative: string;
  notes: string;
  confidence: number;
  isLocked: boolean;
}

export interface BpmView {
  label: string;
  alternate: string | null;
  confidence: number;
  isLocked: boolean;
}

export interface HistoryRowView {
  time: string;
  key: string;
  bpm: string;
  copyText: string;
}

/** Everything the overlay renders, derived from a session's state. */
export interface OverlayView {
  status: string;
  statusTone: StatusTone;
  key: KeyView | null;
  bpm: BpmView | null;
  tuning: string | null;
  pill: string;
  history: HistoryRowView[];
}

function keyView(params: { estimate: Estimate; isLocked: boolean }): KeyView | null {
  const { estimate, isLocked } = params;
  if (estimate.key === null) return null;
  const key = parseKey(estimate.key);
  if (!key) return null;
  return {
    label: estimate.key,
    relative: shortKeyLabel(relativeKey(key)),
    notes: scaleNotes(key).join(' '),
    confidence: estimate.keyConfidence,
    isLocked,
  };
}

/** `½ 75` or `×2 300`, depending on which side of the main tempo the alternate sits. */
export function formatBpmAlternate(params: { bpm: number; alternate: number }): string {
  const prefix = params.alternate < params.bpm ? '½' : '×2';
  return `${prefix} ${String(Math.round(params.alternate))}`;
}

function bpmView(params: { estimate: Estimate; isLocked: boolean }): BpmView | null {
  const { estimate, isLocked } = params;
  if (estimate.bpm === null) return null;
  const alternate =
    estimate.bpmAlternate === null
      ? null
      : formatBpmAlternate({ bpm: estimate.bpm, alternate: estimate.bpmAlternate });
  return {
    label: String(Math.round(estimate.bpm)),
    alternate,
    confidence: estimate.bpmConfidence,
    isLocked,
  };
}

/** `+23¢` when the offset is large enough to matter, otherwise null. */
export function formatTuning(cents: number | null): string | null {
  if (cents === null || Math.abs(cents) < TUNING_DISPLAY_CENTS) return null;
  const rounded = Math.round(cents);
  return `${rounded > 0 ? '+' : '-'}${String(Math.abs(rounded))}¢`;
}

function roundedBpm(estimate: Estimate | null): string | null {
  const bpm = estimate?.bpm ?? null;
  return bpm === null ? null : String(Math.round(bpm));
}

/** `F# major · 142 BPM`, leaving out whatever is missing. */
export function estimateCopyText(estimate: Estimate | null): string {
  const bpm = roundedBpm(estimate);
  const parts: string[] = [];
  if (estimate?.key) parts.push(estimate.key);
  if (bpm !== null) parts.push(`${bpm} BPM`);
  return parts.length > 0 ? parts.join(' · ') : EMPTY;
}

function formatClock(timestamp: number): string {
  const date = new Date(timestamp);
  const pad = (value: number): string => String(value).padStart(2, '0');
  return `${pad(date.getHours())}:${pad(date.getMinutes())}`;
}

function historyRow(entry: HistoryEntry): HistoryRowView {
  const { estimate } = entry;
  return {
    time: formatClock(entry.endedAt),
    key: estimate?.key ?? EMPTY,
    bpm: roundedBpm(estimate) ?? EMPTY,
    copyText: estimateCopyText(estimate),
  };
}

function statusLine(params: {
  state: SessionState;
  estimate: Estimate | null;
  isLocked: boolean;
}): {
  status: string;
  statusTone: StatusTone;
} {
  const { state, estimate, isLocked } = params;
  if (state.error !== null) return { status: state.error, statusTone: 'error' };
  if (state.window === null) return { status: 'listening…', statusTone: 'idle' };
  if (state.window.hasEnded) return { status: 'silence', statusTone: 'idle' };
  if (isLocked) return { status: 'locked', statusTone: 'locked' };
  const seconds = estimate ? ` ${String(Math.round(estimate.secondsAnalyzed))}s` : '';
  return { status: `analyzing${seconds}`, statusTone: 'active' };
}

function pillText(params: { key: KeyView | null; bpm: BpmView | null }): string {
  const parsedKey = params.key ? parseKey(params.key.label) : null;
  const keyLabel = parsedKey ? shortKeyLabel(parsedKey) : EMPTY;
  return `${params.bpm?.label ?? EMPTY} · ${keyLabel}`;
}

export function overlayView(state: SessionState): OverlayView {
  const estimates = state.window?.recentEstimates ?? [];
  const estimate = latestEstimate(state.window);
  const key = estimate ? keyView({ estimate, isLocked: isKeyLocked(estimates) }) : null;
  const bpm = estimate ? bpmView({ estimate, isLocked: isBpmLocked(estimates) }) : null;
  // Locked once every value that is there has locked; a beat with no key can still lock.
  const isLocked =
    (key !== null || bpm !== null) && (key?.isLocked ?? true) && (bpm?.isLocked ?? true);

  return {
    ...statusLine({ state, estimate, isLocked }),
    key,
    bpm,
    tuning: formatTuning(estimate?.tuningCents ?? null),
    pill: pillText({ key, bpm }),
    history: state.history.map(historyRow),
  };
}
