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
