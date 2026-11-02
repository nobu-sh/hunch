// Loads the built extension into Chrome for Testing, turns it on for a page, and reports what the
// overlay shows. Run `pnpm browser --help` for options.
import { resolve as resolvePath } from 'node:path';
import { parseArgs } from 'node:util';
import { openBrowser } from '../browser.mjs';
import { collectConsoleErrors } from '../console.mjs';
import { saveScreenshot } from '../screenshot.mjs';
import { startTestPage, TEST_BEAT } from '../test-page.mjs';
import { extensionUrlPrefix, installExtension } from './driver.mjs';
import { checkTestBeat, formatReadout, readOverlay } from './overlay.mjs';

const DEFAULT_EXTENSION_DIR = '.output/chrome-mv3';
const POLL_INTERVAL_MS = 500;

const HELP = `Usage: pnpm browser [options]

Loads extension/.output/chrome-mv3 into Chrome for Testing, opens a page, clicks the Hunch
toolbar icon on it, and prints the overlay's readouts as they change.

Options:
  --url <url>          Page to open. Default: a local page playing a ${String(TEST_BEAT.bpm)} BPM ${TEST_BEAT.scale} beat.
  --seconds <n>        How long to listen before finishing. Default: 12.
  --headed             Show the browser window. Default: headless.
  --keep-open          With --headed, leave the browser open until you close it.
  --screenshot <file>  Save a screenshot of the page, overlay included, at the end.
  --check              Exit 1 unless the test beat's key and BPM show and nothing logged an error.
  --connect <target>   Attach to a running browser instead of launching one: a debugging port,
                       an http(s) DevTools URL, or a ws(s) endpoint. It needs
                       --enable-unsafe-extension-debugging to install the extension.
  --extension <dir>    Unpacked build to load. Default: ${DEFAULT_EXTENSION_DIR}, from the current folder.
  --help               Show this help.

Needs a build (pnpm build) and Chrome for Testing (pnpm browser:install).
Audio plays through the speakers while it runs: muting Chrome would also mute the capture.`;

function parseOptions(args) {
  const { values } = parseArgs({
    args,
    options: {
      url: { type: 'string' },
      seconds: { type: 'string', default: '12' },
      headed: { type: 'boolean', default: false },
      'keep-open': { type: 'boolean', default: false },
      screenshot: { type: 'string' },
      check: { type: 'boolean', default: false },
      connect: { type: 'string' },
      extension: { type: 'string', default: DEFAULT_EXTENSION_DIR },
      help: { type: 'boolean', default: false },
    },
  });
  const seconds = Number(values.seconds);
  if (!Number.isFinite(seconds) || seconds <= 0) {
    throw new Error(`--seconds must be a positive number, got "${values.seconds}"`);
  }
  if (values.check && values.url) {
    throw new Error('--check only works with the built-in test page; drop --url');
  }
  if (values.connect && values.headed) {
    throw new Error('--headed only applies to a launched browser; drop it with --connect');
  }
  return { ...values, seconds };
}

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
  const options = parseOptions(args);
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
  const session = await openBrowser({
    connect: options.connect,
    headed: options.headed,
    enableExtensions: true,
  });
  const { browser } = session;
  let page = null;

  try {
    const extension = await installExtension({ browser, extensionDir: options.extension });
    // An attached browser may be someone's own window, so leave its tabs alone.
    page = session.isConnected
      ? await browser.newPage()
      : ((await browser.pages())[0] ?? (await browser.newPage()));
    const errors = collectConsoleErrors({
      browser,
      page,
      targetUrlPrefix: extensionUrlPrefix(extension),
    });

    const url = options.url ?? testPage.url;
    console.log(`Opening ${url}`);
    await page.goto(url);
    if (testPage) {
      await page.waitForFunction(() => document.querySelector('audio')?.paused === false);
    }
    await extension.triggerAction(page);

    const overlay = await listen({ page, seconds: options.seconds });
    if (options.screenshot) {
      // pnpm runs scripts from the package folder; INIT_CWD is where the command was typed.
      const path = resolvePath(process.env.INIT_CWD ?? process.cwd(), options.screenshot);
      await saveScreenshot({ page, path });
    }
    for (const error of errors) console.log(`error  ${error}`);
    const exitCode = options.check ? reportCheck({ overlay, errors }) : 0;

    if (options.headed && options['keep-open']) {
      console.log('Browser left open; close it to exit.');
      await new Promise((resolve) => browser.once('disconnected', resolve));
    }
    return exitCode;
  } finally {
    if (session.isConnected && page && !page.isClosed()) await page.close();
    await session.close();
  }
}
