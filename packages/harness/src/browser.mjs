// Starts Chrome for Testing, or attaches to a browser that's already running with a DevTools
// endpoint, such as the desktop app's WebView2 on its remote debugging port.
import puppeteer from 'puppeteer-core';
import { chromeExecutable } from './chrome.mjs';

const LOCAL_HOST = '127.0.0.1';
const HEADLESS_VIEWPORT = { width: 1280, height: 800 };

/**
 * Launches Chrome for Testing, or connects to `connect` when given: a port, an http(s) DevTools
 * URL, or a ws(s) browser endpoint. `close` shuts a launched browser and only detaches from a
 * connected one.
 */
export async function openBrowser({ connect, headed = false, enableExtensions = false }) {
  if (connect) {
    const browser = await puppeteer.connect({ ...connectTarget(connect), defaultViewport: null });
    return { browser, isConnected: true, close: () => browser.disconnect() };
  }

  const browser = await puppeteer.launch({
    executablePath: await chromeExecutable(),
    headless: !headed,
    enableExtensions,
    pipe: true,
    defaultViewport: headed ? null : HEADLESS_VIEWPORT,
    // A muted browser also mutes tab capture, so pages play out loud, even headless.
    ignoreDefaultArgs: ['--mute-audio'],
    args: ['--autoplay-policy=no-user-gesture-required'],
  });
  return {
    browser,
    isConnected: false,
    close: async () => {
      if (browser.connected) await browser.close();
    },
  };
}

function connectTarget(connect) {
  const value = String(connect);
  if (/^\d+$/.test(value)) return { browserURL: `http://${LOCAL_HOST}:${value}` };
  if (/^wss?:\/\//.test(value)) return { browserWSEndpoint: value };
  if (/^https?:\/\//.test(value)) return { browserURL: value };
  throw new Error(`Can't connect to "${value}": expected a port, an http(s) URL or a ws(s) URL`);
}
