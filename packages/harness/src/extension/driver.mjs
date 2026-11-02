import { existsSync } from 'node:fs';
import { resolve } from 'node:path';

/** Mirrors `storageKey` in extension/src/lib/tab-store.ts. */
const TAB_STATE_KEY_PREFIX = 'tabState:';

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
  await serviceWorker({ browser, extension });
  return extension;
}

/** The prefix every page and worker of the extension shares. */
export function extensionUrlPrefix(extension) {
  return `chrome-extension://${extension.id}/`;
}

/** Turns Hunch on for `page` by clicking its toolbar icon, once the service worker is running. */
export async function turnOn({ browser, extension, page }) {
  // The click is lost if it lands before the service worker is running.
  await serviceWorker({ browser, extension });
  await extension.triggerAction(page);
}

/**
 * The background's session state for the tab showing `pageUrl`, read from the extension's own
 * `chrome.storage.session`, or null when it has none.
 */
export async function readTabSession({ browser, extension, pageUrl }) {
  const worker = await serviceWorker({ browser, extension });
  return worker.evaluate(
    async ({ url, keyPrefix }) => {
      const tabs = await chrome.tabs.query({});
      const tab = tabs.find((candidate) => candidate.url === url);
      if (tab?.id === undefined) return null;
      const key = `${keyPrefix}${String(tab.id)}`;
      const stored = await chrome.storage.session.get(key);
      return stored[key] ?? null;
    },
    { url: pageUrl, keyPrefix: TAB_STATE_KEY_PREFIX },
  );
}

async function serviceWorker({ browser, extension }) {
  const prefix = extensionUrlPrefix(extension);
  const target = await browser.waitForTarget(
    (candidate) => candidate.type() === 'service_worker' && candidate.url().startsWith(prefix),
  );
  return target.worker();
}
