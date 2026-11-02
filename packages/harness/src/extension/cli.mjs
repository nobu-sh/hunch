// Loads the built extension into Chrome for Testing, turns Hunch on for a page, and reports the
// extension's session state for it. Run `pnpm browser --help` for options.
import { clearEndpoint, ENDPOINT_FILE, recordEndpoint } from '../endpoint.mjs';
import { startTestPage, TEST_BEAT, waitForTestBeat } from '../test-page.mjs';
import { readTabSession, turnOn } from './driver.mjs';
import { openExtensionSession } from './launch.mjs';
import { DEFAULT_CHECK_SECONDS, DEFAULT_EXTENSION_DIR, parseExtensionOptions } from './options.mjs';
import { checkTestBeatSession, formatSession } from './tab-session.mjs';

const POLL_INTERVAL_MS = 500;
/** How long a failed read waits to see whether the browser is closing rather than broken. */
const DISCONNECT_GRACE_MS = 2_000;

const HELP = `Usage: pnpm browser [options]

Loads extension/.output/chrome-mv3 into Chrome for Testing, opens a page, turns Hunch on for it
through the toolbar icon, and prints the extension's session state for the tab as it changes.
It stays open until you close the browser, with its DevTools endpoint in ${ENDPOINT_FILE}
so Puppeteer scripts can attach. Console errors print as they happen.

Options:
  --url <url>          Page to open. Default: a local page playing a ${String(TEST_BEAT.bpm)} BPM ${TEST_BEAT.scale} beat.
  --headless           Run without a window, for unattended checks. Default: a visible window.
  --seconds <n>        Close after n seconds instead of staying open.
  --check              Exit 1 unless the test beat reached the analyzer: the session has a window
                       with estimates, a plausible tempo and no errors. Listens ${String(DEFAULT_CHECK_SECONDS)} s by default.
  --connect <target>   Attach to a running browser instead of launching one: a debugging port,
                       an http(s) DevTools URL, or a ws(s) endpoint. It needs
                       --enable-unsafe-extension-debugging to install the extension.
  --extension <dir>    Unpacked build to load. Default: ${DEFAULT_EXTENSION_DIR}, from the current folder.
  --help               Show this help.

Needs a build (pnpm build) and Chrome for Testing (pnpm browser:install).
Audio plays through the speakers while it runs: muting Chrome would also mute the capture.`;

/**
 * Polls the session state, printing it when it changes, until `seconds` pass or the browser
 * disconnects (`disconnected` settles).
 */
async function watch({ readState, seconds, disconnected }) {
  let isDisconnected = false;
  void disconnected.then(() => {
    isDisconnected = true;
  });
  const isDone = () => isDisconnected;
  let state = null;
  let lastReadout = '';
  const startedAt = Date.now();
  const hasTimeLeft = () => seconds === null || Date.now() - startedAt < seconds * 1000;
  while (hasTimeLeft() && !isDone()) {
    await delay(POLL_INTERVAL_MS);
    if (isDone()) break;
    try {
      state = await readState();
    } catch (error) {
      // Closing the browser is how a run ends; Chrome fails the read just before it disconnects.
      await Promise.race([disconnected, delay(DISCONNECT_GRACE_MS)]);
      if (isDone()) break;
      throw error;
    }
    const readout = formatSession(state);
    if (readout !== lastReadout) {
      const elapsed = ((Date.now() - startedAt) / 1000).toFixed(1);
      console.log(`${elapsed.padStart(5)}s  ${readout}`);
      lastReadout = readout;
    }
  }
  return state;
}

function delay(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function reportCheck({ state, errors }) {
  const problems = checkTestBeatSession({ state, errors });
  for (const problem of problems) console.log(`FAIL  ${problem}`);
  if (problems.length === 0) console.log('PASS  test beat reached the analyzer');
  return problems.length === 0 ? 0 : 1;
}

/** Runs the extension harness with command-line `args` and resolves to its exit code. */
export async function runExtension(args) {
  const options = parseExtensionOptions(args);
  if (options.help) {
    console.log(HELP);
    return 0;
  }

  const testPage = options.url ? null : await startTestPage();
  try {
    return await drive({ options, testPage });
  } finally {
    await testPage?.close();
  }
}

async function drive({ options, testPage }) {
  let isClosing = false;
  const { browser, isConnected, extension, page, errors, close } = await openExtensionSession({
    connect: options.connect,
    headless: options.headless,
    extensionDir: options.extension,
    url: options.url ?? testPage.url,
    // Shutting down makes the extension log errors about its own teardown; they aren't news.
    onError: (error) => {
      if (!isClosing) console.log(`error  ${error}`);
    },
  });
  const disconnected = new Promise((resolve) => browser.once('disconnected', resolve));

  try {
    console.log(`DevTools: ${browser.wsEndpoint()}`);
    // An attached browser has its own endpoint; only a launched one is ours to record.
    if (!isConnected) await recordEndpoint(browser.wsEndpoint());
    if (testPage) await waitForTestBeat(page);
    await turnOn({ browser, extension, page });
    if (options.seconds === null) console.log('Hunch is on. Close the browser to exit.');

    const pageUrl = page.url();
    const state = await watch({
      readState: () => readTabSession({ browser, extension, pageUrl }),
      seconds: options.seconds,
      disconnected,
    });
    return options.check ? reportCheck({ state, errors }) : 0;
  } finally {
    isClosing = true;
    if (!isConnected) await clearEndpoint();
    await close();
  }
}
