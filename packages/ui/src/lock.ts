import type { Estimate } from './estimate';
import { parseKey, scaleId } from './music';

// Picked on validation timelines (`hunch-eval timeline`), with estimates every 0.5 s.
// Confidence carries most of the signal; the streak only filters one-off blips.
export const BPM_LOCK_STREAK = 2;
export const BPM_LOCK_CONFIDENCE = 0.65;
/** BPMs within this fraction of each other count as the same tempo. */
export const BPM_SAME_TOLERANCE = 0.02;
export const KEY_LOCK_STREAK = 3;
export const KEY_LOCK_CONFIDENCE = 0.6;

/** The last `length` values, or null when there are fewer. Newest last. */
function streakOf<T>(params: { values: T[]; length: number }): T[] | null {
  const streak = params.values.slice(-params.length);
  return streak.length < params.length ? null : streak;
}

/** BPM locks when confident and steady (within `BPM_SAME_TOLERANCE`) for `BPM_LOCK_STREAK` estimates. Newest last. */
export function isBpmLocked(estimates: Estimate[]): boolean {
  const latest = estimates.at(-1);
  if (!latest || latest.bpm === null || latest.bpmConfidence < BPM_LOCK_CONFIDENCE) return false;
  const latestBpm = latest.bpm;
  const streak = streakOf({
    values: estimates.map((estimate) => estimate.bpm),
    length: BPM_LOCK_STREAK,
  });
  return (
    streak !== null &&
    streak.every(
      (bpm) => bpm !== null && Math.abs(bpm - latestBpm) <= BPM_SAME_TOLERANCE * latestBpm,
    )
  );
}

/**
 * Key locks when confident and its scale held for `KEY_LOCK_STREAK` estimates. Newest last.
 * Flipping between relative keys keeps the streak: they're the same notes for autotune.
 */
export function isKeyLocked(estimates: Estimate[]): boolean {
  const latest = estimates.at(-1);
  if (!latest || latest.keyConfidence < KEY_LOCK_CONFIDENCE) return false;
  const scales = estimates.map((estimate) => {
    const key = estimate.key === null ? null : parseKey(estimate.key);
    return key === null ? null : scaleId(key);
  });
  const streak = streakOf({ values: scales, length: KEY_LOCK_STREAK });
  const [first] = streak ?? [];
  return streak !== null && first !== null && streak.every((scale) => scale === first);
}
