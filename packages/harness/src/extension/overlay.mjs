// Reading and judging what the Hunch overlay shows.
import { callInShadowRoot, clickInShadowRoot } from '../devtools.mjs';
import { TEST_BEAT } from '../test-page.mjs';

export const OVERLAY_TAG = 'hunch-overlay';
/** Keys that count as right for the test beat: A minor or its relative, C major. */
const TEST_BEAT_KEYS = ['A minor', 'C major'];
const BPM_TOLERANCE = 0.04;
const WAIT_POLL_MS = 200;

const READ_OVERLAY = `function () {
  const text = (ref) => this.querySelector('[data-ref="' + ref + '"]')?.textContent.trim() || null;
  const pill = this.querySelector('[data-ref="pill"]');
  return {
    isCollapsed: pill !== null && pill.getBoundingClientRect().width > 0,
    status: text('status'),
    key: text('key'),
    relative: text('relative'),
    tuning: text('tuning'),
    bpm: text('bpm'),
    alternate: text('alternate'),
    notes: text('notes'),
    history: [...this.querySelectorAll('.history li')].map((row) => row.textContent.trim()),
  };
}`;

/** The overlay's readouts, or null when it isn't mounted. Its shadow root is closed to pages. */
export function readOverlay(page) {
  return callInShadowRoot({ page, hostName: OVERLAY_TAG, functionDeclaration: READ_OVERLAY });
}

/** Clicks the overlay control marked `data-ref="<ref>"`, such as `collapse` or `pill`. */
export function clickOverlay({ page, ref }) {
  return clickInShadowRoot({ page, hostName: OVERLAY_TAG, selector: `[data-ref="${ref}"]` });
}

/**
 * Polls the overlay until `isReady(overlay)` holds and returns that reading. Throws after
 * `timeoutMs`, naming what it waited for and what the overlay last showed.
 */
export async function waitForOverlay({ page, isReady, description, timeoutMs }) {
  const deadline = Date.now() + timeoutMs;
  let overlay = null;
  while (Date.now() < deadline) {
    overlay = await readOverlay(page);
    if (overlay && isReady(overlay)) return overlay;
    await new Promise((resolve) => setTimeout(resolve, WAIT_POLL_MS));
  }
  throw new Error(
    `Timed out after ${String(timeoutMs)} ms waiting for ${description}; overlay showed: ${formatReadout(overlay)}`,
  );
}

export function formatReadout(overlay) {
  if (!overlay) return 'overlay not mounted';
  const key = overlay.key ? `${overlay.key}${overlay.tuning ? ` ${overlay.tuning}` : ''}` : '—';
  const bpm = overlay.bpm
    ? `${overlay.bpm}${overlay.alternate ? ` (${overlay.alternate})` : ''}`
    : '—';
  return `${overlay.status ?? ''} | key ${key} | bpm ${bpm} | history ${String(overlay.history.length)}`;
}

/** What's wrong with the overlay's read of the test beat; empty when it's right. */
export function checkTestBeat({ overlay, errors }) {
  const problems = [];
  if (!overlay) problems.push('the overlay never mounted');
  if (overlay && !TEST_BEAT_KEYS.includes(overlay.key ?? '')) {
    problems.push(`key is "${overlay.key ?? '—'}", expected ${TEST_BEAT_KEYS.join(' or ')}`);
  }
  const bpm = Number(overlay?.bpm);
  if (overlay && !(Math.abs(bpm - TEST_BEAT.bpm) <= TEST_BEAT.bpm * BPM_TOLERANCE)) {
    problems.push(`bpm is "${overlay.bpm ?? '—'}", expected ${String(TEST_BEAT.bpm)}`);
  }
  for (const error of errors) problems.push(`console error: ${error}`);
  return problems;
}
