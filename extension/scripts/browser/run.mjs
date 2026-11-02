// Loads the built extension into Chrome for Testing, turns it on for a page, and reports what the
// overlay shows. Run `pnpm browser --help` for options.
import { existsSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';
import puppeteer from 'puppeteer-core';
import { chromeExecutable } from './chrome.mjs';
import { startTestPage, TEST_BEAT } from './test-page.mjs';

const EXTENSION_DIR = fileURLToPath(new URL('../../.output/chrome-mv3', import.meta.url));
const POLL_INTERVAL_MS = 500;
/** Keys that count as right for the test beat: A minor or its relative, C major. */
const TEST_BEAT_KEYS = ['A minor', 'C major'];
const BPM_TOLERANCE = 0.04;

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
  --help               Show this help.

Needs a build (pnpm build) and Chrome for Testing (pnpm browser:install).
Audio plays through the speakers while it runs: muting Chrome would also mute the capture.`;

function parseOptions() {
  const { values } = parseArgs({
    options: {
      url: { type: 'string' },
      seconds: { type: 'string', default: '12' },
      headed: { type: 'boolean', default: false },
      'keep-open': { type: 'boolean', default: false },
      screenshot: { type: 'string' },
      check: { type: 'boolean', default: false },
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
  return { ...values, seconds };
}

/** Reads the overlay through DevTools, since its shadow root is closed to page scripts. */
async function readOverlay(page) {
  const session = await page.createCDPSession();
  try {
    const { root } = await session.send('DOM.getDocument', { depth: -1, pierce: true });
    const shadowRoot = findOverlayShadowRoot(root);
    if (!shadowRoot) return null;
    const { object } = await session.send('DOM.resolveNode', {
      backendNodeId: shadowRoot.backendNodeId,
    });
    const { result } = await session.send('Runtime.callFunctionOn', {
      objectId: object.objectId,
      returnByValue: true,
      functionDeclaration: `function () {
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
      }`,
    });
    return result.value;
  } finally {
    await session.detach();
  }
}

function findOverlayShadowRoot(node) {
  if (node.localName === 'hunch-overlay') return node.shadowRoots?.[0] ?? null;
  for (const child of [...(node.children ?? []), ...(node.shadowRoots ?? [])]) {
    const found = findOverlayShadowRoot(child);
    if (found) return found;
  }
  return null;
}

/** Collects console errors from the page, the service worker, and the offscreen document. */
function collectErrors({ browser, page, extensionId }) {
  const errors = [];
  const record = (source) => (message) => {
    if (message.type() === 'error') errors.push(`${source}: ${message.text()}`);
  };
  page.on('console', record('page'));
  page.on('pageerror', (error) => errors.push(`page: ${error.message}`));

  const watchTarget = async (target) => {
    if (!target.url().startsWith(`chrome-extension://${extensionId}/`)) return;
    const name = new URL(target.url()).pathname.slice(1);
    if (target.type() === 'service_worker') {
      (await target.worker())?.on('console', record(name));
      return;
    }
    const targetPage = await target.asPage().catch(() => null);
    targetPage?.on('console', record(name));
  };
  for (const target of browser.targets()) void watchTarget(target);
  browser.on('targetcreated', (target) => void watchTarget(target));
  return errors;
}

function formatReadout(overlay) {
  if (!overlay) return 'overlay not mounted';
  const key = overlay.key ? `${overlay.key}${overlay.tuning ? ` ${overlay.tuning}` : ''}` : '—';
  const bpm = overlay.bpm
    ? `${overlay.bpm}${overlay.alternate ? ` (${overlay.alternate})` : ''}`
    : '—';
  return `${overlay.status ?? ''} | key ${key} | bpm ${bpm} | history ${String(overlay.history.length)}`;
}

function checkTestBeat({ overlay, errors }) {
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

async function main() {
  const options = parseOptions();
  if (options.help) {
    console.log(HELP);
    return 0;
  }
  if (!existsSync(`${EXTENSION_DIR}/manifest.json`)) {
    throw new Error(`No extension build at ${EXTENSION_DIR}. Run: pnpm build`);
  }

  const testPage = options.url ? null : await startTestPage();
  const browser = await puppeteer.launch({
    executablePath: await chromeExecutable(),
    headless: !options.headed,
    enableExtensions: true,
    pipe: true,
    defaultViewport: options.headed ? null : { width: 1280, height: 800 },
    // A muted browser also mutes tab capture, so the page plays out loud, even headless.
    ignoreDefaultArgs: ['--mute-audio'],
    args: ['--autoplay-policy=no-user-gesture-required'],
  });

  try {
    // Installed explicitly: passing the path to `launch` doesn't finish loading it.
    const extensionId = await browser.installExtension(EXTENSION_DIR);
    const extension = (await browser.extensions()).get(extensionId);
    if (!extension) throw new Error(`Chrome didn't list the installed extension ${extensionId}`);
    const page = (await browser.pages())[0] ?? (await browser.newPage());
    const errors = collectErrors({ browser, page, extensionId: extension.id });

    // The toolbar click is lost if it lands before the service worker is running.
    await browser.waitForTarget(
      (target) =>
        target.type() === 'service_worker' &&
        target.url().startsWith(`chrome-extension://${extensionId}/`),
    );
    const url = options.url ?? testPage.url;
    console.log(`Opening ${url}`);
    await page.goto(url);
    if (testPage) {
      await page.waitForFunction(() => document.querySelector('audio')?.paused === false);
    }
    await extension.triggerAction(page);

    let overlay = null;
    let lastReadout = '';
    const startedAt = Date.now();
    while (Date.now() - startedAt < options.seconds * 1000) {
      await new Promise((resolve) => setTimeout(resolve, POLL_INTERVAL_MS));
      overlay = await readOverlay(page);
      const readout = formatReadout(overlay);
      if (readout !== lastReadout) {
        const elapsed = ((Date.now() - startedAt) / 1000).toFixed(1);
        console.log(`${elapsed.padStart(5)}s  ${readout}`);
        lastReadout = readout;
      }
    }

    if (options.screenshot) {
      await page.screenshot({ path: options.screenshot });
      console.log(`Screenshot: ${options.screenshot}`);
    }
    for (const error of errors) console.log(`error  ${error}`);

    let exitCode = 0;
    if (options.check) {
      const problems = checkTestBeat({ overlay, errors });
      for (const problem of problems) console.log(`FAIL  ${problem}`);
      console.log(problems.length === 0 ? 'PASS  test beat detected' : '');
      exitCode = problems.length === 0 ? 0 : 1;
    }

    if (options.headed && options['keep-open']) {
      console.log('Browser left open; close it to exit.');
      await new Promise((resolve) => browser.once('disconnected', resolve));
    }
    return exitCode;
  } finally {
    if (browser.connected) await browser.close();
    await testPage?.close();
  }
}

try {
  process.exitCode = await main();
} catch (error) {
  console.error(error instanceof Error ? error.message : error);
  process.exitCode = 1;
}
