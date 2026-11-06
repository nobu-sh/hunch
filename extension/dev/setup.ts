// What the Hunch mixins do: play a test page, turn Hunch on, and follow the tab's session.
import type { LoadedExtension, MixinContext } from '@hunch/browser';
import type { SessionEvent, SessionState } from '@hunch/ui';
import { type Browser, TargetType, type WebWorker } from 'puppeteer-core';
import { formatSession } from './session.ts';
import { type TestBeatServer, waitForTestBeat } from './test-beat.ts';

/** The unpacked build, relative to this folder. */
export const BUILD_DIR = '../.output/chrome-mv3';
/** Mirrors `storageKey` in extension/src/lib/tab-store.ts. */
const TAB_STATE_KEY_PREFIX = 'tabState:';
const POLL_INTERVAL_MS = 500;
const MS_PER_SECOND = 1000;
/** How long a failed read waits to see whether the browser is closing rather than broken. */
const CLOSE_GRACE_MS = 2_000;

/** Minimal shape of the extension APIs the mixins use in the service worker. */
interface ExtensionApis {
  tabs: { query: (query: object) => Promise<{ id?: number; url?: string }[]> };
  storage: { session: { get: (key: string) => Promise<Record<string, unknown>> } };
  runtime: { onMessage: { addListener: (listener: (message: unknown) => void) => void } };
}

/**
 * Waits for the test beat when the page is showing it, then clicks Hunch's toolbar icon.
 * Returns a reader for the tab's session state.
 */
export async function turnOnHunch(params: {
  context: MixinContext;
  testBeat: TestBeatServer;
}): Promise<() => Promise<SessionState | null>> {
  const { context, testBeat } = params;
  const { browser, page } = context;
  const hunch = hunchExtension(context.extensions);
  if (page.url() === testBeat.url) await waitForTestBeat(page);
  await hunch.handle.triggerAction(page);
  context.log('Hunch is on.');

  const pageUrl = page.url();
  const worker = await hunchWorker({ browser, hunch });
  return () => readTabSession({ worker, pageUrl });
}

async function hunchWorker(params: {
  browser: Browser;
  hunch: LoadedExtension;
}): Promise<WebWorker> {
  const { browser, hunch } = params;
  const workerTarget = await browser.waitForTarget(
    (target) =>
      target.type() === TargetType.SERVICE_WORKER && target.url().startsWith(hunch.urlPrefix),
  );
  const worker = await workerTarget.worker();
  if (!worker) throw new Error(`Hunch's service worker at ${workerTarget.url()} isn't running`);
  return worker;
}

/** The window start and end events the service worker has heard, kept by `recordWindowEvents`. */
const WINDOW_EVENTS_GLOBAL = '__hunchDevWindowEvents';

/**
 * Keeps every `windowStarted` and `windowEnded` session event Hunch's service worker receives
 * from now on, since the stored session state doesn't say why windows started or ended. Returns
 * a reader for them, oldest first.
 */
export async function recordWindowEvents(
  context: MixinContext,
): Promise<() => Promise<SessionEvent[]>> {
  const worker = await hunchWorker({
    browser: context.browser,
    hunch: hunchExtension(context.extensions),
  });
  await worker.evaluate((globalName) => {
    const scope = globalThis as unknown as Record<string, unknown> & { chrome: ExtensionApis };
    const events: unknown[] = [];
    scope[globalName] = events;
    scope.chrome.runtime.onMessage.addListener((message) => {
      const { type, event } = message as { type?: string; event?: { type?: string } };
      const isWindowEvent = event?.type === 'windowStarted' || event?.type === 'windowEnded';
      if (type === 'session-event' && isWindowEvent) events.push(event);
    });
  }, WINDOW_EVENTS_GLOBAL);
  return () =>
    worker.evaluate(
      (globalName) => (globalThis as unknown as Record<string, SessionEvent[]>)[globalName] ?? [],
      WINDOW_EVENTS_GLOBAL,
    );
}

function hunchExtension(extensions: LoadedExtension[]): LoadedExtension {
  const [hunch] = extensions;
  if (!hunch) {
    throw new Error('No extension loaded: the Hunch mixins need its build, so run pnpm build');
  }
  return hunch;
}

/** The background's state for the tab at `pageUrl`, from Hunch's own `chrome.storage.session`. */
function readTabSession(params: {
  worker: WebWorker;
  pageUrl: string;
}): Promise<SessionState | null> {
  return params.worker.evaluate(
    async ({ url, keyPrefix }) => {
      const { chrome } = globalThis as unknown as { chrome: ExtensionApis };
      const tabs = await chrome.tabs.query({});
      const tab = tabs.find((candidate) => candidate.url === url);
      if (tab?.id === undefined) return null;
      const key = `${keyPrefix}${String(tab.id)}`;
      const stored = await chrome.storage.session.get(key);
      return (stored[key] as SessionState | undefined) ?? null;
    },
    { url: params.pageUrl, keyPrefix: TAB_STATE_KEY_PREFIX },
  );
}

/**
 * Prints the session state whenever it changes, until `context.signal` aborts. Resolves to the
 * last state read.
 */
export async function followSession(params: {
  context: MixinContext;
  readState: () => Promise<SessionState | null>;
}): Promise<SessionState | null> {
  const { context, readState } = params;
  // A function, so the check after each wait sees an abort that happened during it.
  const isStopped = () => context.signal.aborted;
  let state: SessionState | null = null;
  let lastReadout = '';
  const startedAt = Date.now();
  while (!isStopped()) {
    await delay(POLL_INTERVAL_MS);
    if (isStopped()) break;
    try {
      state = await readState();
    } catch (error) {
      // Closing the browser fails the read just before the signal aborts.
      await abortedWithin({ signal: context.signal, ms: CLOSE_GRACE_MS });
      if (isStopped()) break;
      throw error;
    }
    const readout = formatSession(state);
    if (readout === lastReadout) continue;
    const elapsed = ((Date.now() - startedAt) / MS_PER_SECOND).toFixed(1);
    context.log(`${elapsed.padStart(5)}s  ${readout}`);
    lastReadout = readout;
  }
  return state;
}

/** Resolves when `signal` aborts or `ms` pass, whichever comes first. */
function abortedWithin(params: { signal: AbortSignal; ms: number }): Promise<void> {
  const { signal, ms } = params;
  if (signal.aborted) return Promise.resolve();
  return new Promise((resolve) => {
    const timer = setTimeout(resolve, ms);
    signal.addEventListener('abort', () => {
      clearTimeout(timer);
      resolve();
    });
  });
}

function delay(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
