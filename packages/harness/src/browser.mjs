// Starts Chrome for Testing, or attaches to a browser that's already running with a DevTools
// endpoint, such as the desktop app's WebView2 on its remote debugging port.
import puppeteer from 'puppeteer-core';
import { chromeExecutable } from './chrome.mjs';

const LOCAL_HOST = '127.0.0.1';
const HEADLESS_VIEWPORT = { width: 1280, height: 800 };

/**
 * Launches Chrome for Testing, in a visible window unless `headless`, or connects to `connect`
 * when given: a port, an http(s) DevTools URL, or a ws(s) browser endpoint. `close` shuts a
 * launched browser and only detaches from a connected one. `browser.wsEndpoint()` is the DevTools
 * endpoint other tools can attach to.
 */
export async function openBrowser({ connect, headless = false, enableExtensions = false }) {
  if (connect) {
    const browser = await puppeteer.connect({ ...connectTarget(connect), defaultViewport: null });
    return { browser, isConnected: true, close: () => browser.disconnect() };
  }

  const browser = await puppeteer.launch({
    executablePath: await chromeExecutable(),
    headless,
    enableExtensions,
    // Over a debugging port rather than a pipe, so other tools can attach to the same browser.
    pipe: false,
    defaultViewport: headless ? HEADLESS_VIEWPORT : null,
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
