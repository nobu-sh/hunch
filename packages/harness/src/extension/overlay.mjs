// Reading and judging what the Hunch overlay shows.
import { callInShadowRoot } from '../devtools.mjs';
import { TEST_BEAT } from '../test-page.mjs';

const OVERLAY_TAG = 'hunch-overlay';
/** Keys that count as right for the test beat: A minor or its relative, C major. */
const TEST_BEAT_KEYS = ['A minor', 'C major'];
const BPM_TOLERANCE = 0.04;

const READ_OVERLAY = `function () {
  const text = (ref) => this.querySelector('[data-ref="' + ref + '"]')?.textContent.trim() || null;
  return {
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
