// Loads the built extension into Chrome for Testing, turns it on for a page, and reports what the
// overlay shows. Run `pnpm browser --help` for options.
import { fromInvocationDir } from '../paths.mjs';
import { saveScreenshot } from '../screenshot.mjs';
import { startTestPage, TEST_BEAT, waitForTestBeat } from '../test-page.mjs';
import { DEFAULT_EXTENSION_DIR, parseExtensionOptions } from './options.mjs';
import { checkTestBeat, formatReadout, readOverlay } from './overlay.mjs';
import { openExtensionSession } from './session.mjs';

const POLL_INTERVAL_MS = 500;

const HELP = `Usage: pnpm browser [options]

Loads extension/.output/chrome-mv3 into Chrome for Testing, opens a page, clicks the Hunch
toolbar icon on it, and prints the overlay's readouts as they change.

Options:
  --url <url>          Page to open. Default: a local page playing a ${String(TEST_BEAT.bpm)} BPM ${TEST_BEAT.scale} beat.
  --seconds <n>        How long to listen before finishing. Default: 12.
  --headless           Run without a window, for unattended checks. Default: a visible window.
  --keep-open          Leave the browser open until you close it.
  --screenshot <file>  Save a screenshot of the page, overlay included, at the end.
  --check              Exit 1 unless the test beat's key and BPM show and nothing logged an error.
  --connect <target>   Attach to a running browser instead of launching one: a debugging port,
                       an http(s) DevTools URL, or a ws(s) endpoint. It needs
                       --enable-unsafe-extension-debugging to install the extension.
  --extension <dir>    Unpacked build to load. Default: ${DEFAULT_EXTENSION_DIR}, from the current folder.
  --help               Show this help.

Needs a build (pnpm build) and Chrome for Testing (pnpm browser:install).
Audio plays through the speakers while it runs: muting Chrome would also mute the capture.`;

async function listen({ page, seconds }) {
  let overlay = null;
  let lastReadout = '';
  const startedAt = Date.now();
  while (Date.now() - startedAt < seconds * 1000) {
    await new Promise((resolve) => setTimeout(resolve, POLL_INTERVAL_MS));
    overlay = await readOverlay(page);
    const readout = formatReadout(overlay);
    if (readout !== lastReadout) {
      const elapsed = ((Date.now() - startedAt) / 1000).toFixed(1);
      console.log(`${elapsed.padStart(5)}s  ${readout}`);
      lastReadout = readout;
    }
  }
  return overlay;
}

function reportCheck({ overlay, errors }) {
  const problems = checkTestBeat({ overlay, errors });
  for (const problem of problems) console.log(`FAIL  ${problem}`);
  console.log(problems.length === 0 ? 'PASS  test beat detected' : '');
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
  const { browser, extension, page, errors, close } = await openExtensionSession({
    connect: options.connect,
    headless: options.headless,
    extensionDir: options.extension,
    url: options.url ?? testPage.url,
  });

  try {
    if (testPage) await waitForTestBeat(page);
    await extension.triggerAction(page);

    const overlay = await listen({ page, seconds: options.seconds });
    if (options.screenshot) {
      await saveScreenshot({ page, path: fromInvocationDir(options.screenshot) });
    }
    for (const error of errors) console.log(`error  ${error}`);
    const exitCode = options.check ? reportCheck({ overlay, errors }) : 0;

    if (options['keep-open']) {
      console.log('Browser left open; close it to exit.');
      await new Promise((resolve) => browser.once('disconnected', resolve));
    }
    return exitCode;
  } finally {
    await close();
  }
}
