// Starts Chrome for Testing, or attaches to a browser that's already running with a DevTools
// endpoint, such as a desktop app's WebView2 on its remote debugging port.
import puppeteer, { type Browser, type LaunchOptions } from 'puppeteer-core';
import { chromeExecutable } from './chrome.ts';
import type { BrowserOptions } from './options.ts';

const LOCAL_HOST = '127.0.0.1';
const HEADLESS_VIEWPORT = { width: 1280, height: 800 };
/** Puppeteer mutes by default, and a muted browser also silences tab capture. */
const MUTE_FLAG = '--mute-audio';
const AUTOPLAY_FLAG = '--autoplay-policy=no-user-gesture-required';

export interface OpenBrowser {
  browser: Browser;
  isConnected: boolean;
  /** Shuts a launched browser; only detaches from a connected one. */
  close: () => Promise<void>;
}

export async function openBrowser(options: BrowserOptions): Promise<OpenBrowser> {
  if (options.connect !== null) {
    const browser = await puppeteer.connect({
      ...connectTarget(options.connect),
      defaultViewport: null,
    });
    return { browser, isConnected: true, close: () => browser.disconnect() };
  }

  const browser = await puppeteer.launch(await launchOptions(options));
  return {
    browser,
    isConnected: false,
    close: async () => {
      if (browser.connected) await browser.close();
    },
  };
}

async function launchOptions(options: BrowserOptions): Promise<LaunchOptions> {
  const { windowSize } = options;
  const args = [AUTOPLAY_FLAG, ...options.chromeArgs];
  if (windowSize) {
    args.push(`--window-size=${String(windowSize.width)},${String(windowSize.height)}`);
  }
  const headlessViewport = windowSize ?? HEADLESS_VIEWPORT;
  return {
    executablePath: await chromeExecutable(),
    headless: options.headless,
    devtools: options.devtools,
    // Extensions load after launch with `installExtension`, which needs this.
    enableExtensions: options.extensions.length > 0,
    // A debugging port rather than a pipe, so other tools can attach to the same browser.
    pipe: false,
    userDataDir: options.profile ?? undefined,
    defaultViewport: options.headless ? headlessViewport : null,
    ignoreDefaultArgs: options.mute ? [] : [MUTE_FLAG],
    args,
  };
}

/** Puppeteer's connect target for a port, an http(s) DevTools URL or a ws(s) endpoint. */
export function connectTarget(connect: string): {
  browserURL?: string;
  browserWSEndpoint?: string;
} {
  if (/^\d+$/.test(connect)) return { browserURL: `http://${LOCAL_HOST}:${connect}` };
  if (/^wss?:\/\//.test(connect)) return { browserWSEndpoint: connect };
  if (/^https?:\/\//.test(connect)) return { browserURL: connect };
  throw new Error(`Can't connect to "${connect}": expected a port, an http(s) URL or a ws(s) URL`);
}
