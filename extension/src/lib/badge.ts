import { latestEstimate, type SessionState } from '@hunch/ui';

export const BADGE_ON_TEXT = 'ON';
export const BADGE_ERROR_TEXT = '!';

/**
 * Toolbar badge for an enabled tab: the rounded BPM once there is one, `ON` while listening
 * without one, `!` on error. Falls back to the last ended window so badge-only pages keep a value.
 */
export function badgeText(state: SessionState): string {
  if (state.error !== null) return BADGE_ERROR_TEXT;
  const estimate = latestEstimate(state.window) ?? state.history[0]?.estimate ?? null;
  const bpm = estimate?.bpm ?? null;
  if (bpm === null) return BADGE_ON_TEXT;
  return String(Math.round(bpm));
}
