import type { Estimate } from './messages';

export const LOCK_STREAK = 3;
export const BPM_LOCK_CONFIDENCE = 0.6;
export const KEY_LOCK_CONFIDENCE = 0.55;

/** True when the last `LOCK_STREAK` values are present and equal. */
function isStable(values: (string | number | null)[]): boolean {
  const streak = values.slice(-LOCK_STREAK);
  if (streak.length < LOCK_STREAK) return false;
  const [first] = streak;
  return first !== null && first !== undefined && streak.every((value) => value === first);
}

/** BPM locks when confident and its rounded value held for `LOCK_STREAK` estimates. Newest last. */
export function isBpmLocked(estimates: Estimate[]): boolean {
  const latest = estimates.at(-1);
  if (!latest || latest.bpmConfidence < BPM_LOCK_CONFIDENCE) return false;
  return isStable(
    estimates.map((estimate) => (estimate.bpm === null ? null : Math.round(estimate.bpm))),
  );
}

/** Key locks when confident and unchanged for `LOCK_STREAK` estimates. Newest last. */
export function isKeyLocked(estimates: Estimate[]): boolean {
  const latest = estimates.at(-1);
  if (!latest || latest.keyConfidence < KEY_LOCK_CONFIDENCE) return false;
  return isStable(estimates.map((estimate) => estimate.key));
}
