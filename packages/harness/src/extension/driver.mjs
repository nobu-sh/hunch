import { existsSync } from 'node:fs';
import { resolve } from 'node:path';

/**
 * Installs the unpacked build at `extensionDir` and waits until its service worker runs.
 * Returns puppeteer's handle for it, which can click its toolbar icon.
 */
export async function installExtension({ browser, extensionDir }) {
  const fullDir = resolve(extensionDir);
  if (!existsSync(`${fullDir}/manifest.json`)) {
    throw new Error(`No extension build at ${fullDir}. Run: pnpm build`);
  }
  // Installed after launch: passing the path to `launch` doesn't finish loading it.
  const extensionId = await browser.installExtension(fullDir);
  const extension = (await browser.extensions()).get(extensionId);
  if (!extension) throw new Error(`Chrome didn't list the installed extension ${extensionId}`);

  // The toolbar click is lost if it lands before the service worker is running.
  await browser.waitForTarget(
    (target) =>
      target.type() === 'service_worker' && target.url().startsWith(extensionUrlPrefix(extension)),
  );
  return extension;
}

/** The prefix every page and worker of the extension shares. */
export function extensionUrlPrefix(extension) {
  return `chrome-extension://${extension.id}/`;
}

/**
 * Makes the extension show a capture error on the tab at `pageUrl`, by sending the background the
 * same message its offscreen document sends when a tab's capture fails.
 */
export async function reportCaptureError({ browser, extension, pageUrl, message }) {
  const prefix = extensionUrlPrefix(extension);
  const workerTarget = await browser.waitForTarget(
    (target) => target.type() === 'service_worker' && target.url().startsWith(prefix),
  );
  const worker = await workerTarget.worker();
  const tabId = await worker.evaluate(async (url) => {
    const tabs = await chrome.tabs.query({});
    return tabs.find((tab) => tab.url?.startsWith(url))?.id ?? null;
  }, pageUrl);
  if (tabId === null) throw new Error(`No tab open at ${pageUrl} to report an error on`);

  // The background ignores messages it sends itself, so this one comes from the offscreen page.
  const offscreenTarget = await browser.waitForTarget((target) =>
    target.url().startsWith(`${prefix}offscreen`),
  );
  const offscreen = await offscreenTarget.asPage();
  await offscreen.evaluate(
    (error) => chrome.runtime.sendMessage({ type: 'capture-error', ...error }),
    { tabId, message },
  );
}
