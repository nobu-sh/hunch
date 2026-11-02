// Walks the overlay through each of its states on the test beat, in a visible window, and saves a
// PNG of each. Run `pnpm browser:screenshots --help` for options.
import { join, resolve } from 'node:path';
import { fromInvocationDir } from '../paths.mjs';
import { saveElementScreenshot, saveScreenshot } from '../screenshot.mjs';
import { pauseTestBeat, playTestBeat, startTestPage, waitForTestBeat } from '../test-page.mjs';
import { reportCaptureError } from './driver.mjs';
import {
  DEFAULT_EXTENSION_DIR,
  DEFAULT_SCREENSHOTS_DIR,
  parseScreenshotsOptions,
} from './options.mjs';
import { clickOverlay, OVERLAY_TAG, waitForOverlay } from './overlay.mjs';
import { openExtensionSession } from './session.mjs';

const HISTORY_ROWS = 3;
const ERROR_MESSAGE = 'The tab audio capture ended';
const STATE_TIMEOUT_MS = 5_000;
const ESTIMATE_TIMEOUT_MS = 15_000;
const LOCK_TIMEOUT_MS = 30_000;
/** Hunch drops windows shorter than 4 s, so each history window plays at least this long. */
const WINDOW_PLAY_MS = 6_000;
/** Windows close after about 2 s of silence; this leaves room for the analysis to catch up. */
const WINDOW_CLOSE_TIMEOUT_MS = 10_000;

const HELP = `Usage: pnpm browser:screenshots [options]

Runs the built extension on the test beat in a visible Chrome for Testing window and saves a PNG
of the overlay in each state: listening, analyzing, locked, collapsed, expanded, history and
error, plus one of the whole page.

Options:
  --out <dir>          Folder for the PNGs. Default: ${DEFAULT_SCREENSHOTS_DIR}, in extension/.
  --extension <dir>    Unpacked build to load. Default: ${DEFAULT_EXTENSION_DIR}, from the current folder.
  --help               Show this help.

Needs a build (pnpm build) and Chrome for Testing (pnpm browser:install).
Audio plays through the speakers while it runs: muting Chrome would also mute the capture.`;

/** Runs the screenshots command with command-line `args` and resolves to its exit code. */
export async function runScreenshots(args) {
  const options = parseScreenshotsOptions(args);
  if (options.help) {
    console.log(HELP);
    return 0;
  }
  const outDir = options.out ? fromInvocationDir(options.out) : resolve(DEFAULT_SCREENSHOTS_DIR);

  const testPage = await startTestPage();
  try {
    const session = await openExtensionSession({
      headless: false,
      extensionDir: options.extension,
      url: testPage.url,
    });
    try {
      const written = await captureStates({ ...session, pageUrl: testPage.url, outDir });
      console.log(`\nWrote ${String(written.length)} screenshots:`);
      for (const path of written) console.log(`  ${path}`);
      return 0;
    } finally {
      await session.close();
    }
  } finally {
    await testPage.close();
  }
}

async function captureStates({ browser, extension, page, pageUrl, outDir }) {
  const written = [];
  const shoot = async (name) => {
    const path = join(outDir, `${name}.png`);
    // Off the overlay, so a button the last click left under the pointer doesn't show its hover.
    await page.mouse.move(0, 0);
    await saveElementScreenshot({ page, selector: OVERLAY_TAG, path });
    written.push(path);
  };
  const waitFor = (description, isReady, timeoutMs = STATE_TIMEOUT_MS) =>
    waitForOverlay({ page, description, isReady, timeoutMs });

  // Paused first, so the overlay sits in "listening" until the beat starts.
  await waitForTestBeat(page);
  await pauseTestBeat(page);
  await extension.triggerAction(page);
  await waitFor('listening', (overlay) => overlay.status?.startsWith('listening') ?? false);
  await shoot('01-listening');

  await playTestBeat(page);
  await waitFor(
    'an early estimate',
    (overlay) => isAnalyzing(overlay) && (overlay.key !== null || overlay.bpm !== null),
    ESTIMATE_TIMEOUT_MS,
  );
  await shoot('02-analyzing');

  await waitFor('a lock', isLocked, LOCK_TIMEOUT_MS);
  await shoot('03-locked');
  const pagePath = join(outDir, '00-page.png');
  await saveScreenshot({ page, path: pagePath });
  written.unshift(pagePath);

  await clickOverlay({ page, ref: 'collapse' });
  await waitFor('the collapsed pill', (overlay) => overlay.isCollapsed);
  await shoot('04-collapsed');

  await clickOverlay({ page, ref: 'pill' });
  await waitFor('the expanded panel', (overlay) => !overlay.isCollapsed);
  await shoot('05-expanded');

  await fillHistory({ page, waitFor });
  await shoot('06-history');

  await reportCaptureError({ browser, extension, pageUrl, message: ERROR_MESSAGE });
  await waitFor('the error', (overlay) => overlay.status === ERROR_MESSAGE);
  await shoot('07-error');
  return written;
}

/** Pauses and resumes the beat until `HISTORY_ROWS` windows have finished. */
async function fillHistory({ page, waitFor }) {
  for (let row = 1; row <= HISTORY_ROWS; row++) {
    if (row > 1) {
      const startedAt = Date.now();
      await playTestBeat(page);
      await waitFor(`window ${String(row)} to lock`, isLocked, LOCK_TIMEOUT_MS);
      const remainingMs = WINDOW_PLAY_MS - (Date.now() - startedAt);
      await new Promise((resolve) => setTimeout(resolve, Math.max(0, remainingMs)));
    }
    await pauseTestBeat(page);
    await waitFor(
      `history row ${String(row)}`,
      (overlay) => overlay.history.length >= row,
      WINDOW_CLOSE_TIMEOUT_MS,
    );
  }
}

function isAnalyzing(overlay) {
  return overlay.status?.startsWith('analyzing') ?? false;
}

function isLocked(overlay) {
  return overlay.status === 'locked';
}
