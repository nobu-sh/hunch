// A browser with the built extension installed and one page to drive it on.
import { openBrowser } from '../browser.mjs';
import { collectConsoleErrors } from '../console.mjs';
import { extensionUrlPrefix, installExtension } from './driver.mjs';

/**
 * Opens a browser (or attaches to `connect`), installs the build at `extensionDir`, and opens
 * `url` in a tab. Console errors from the page and the extension stream to `onError`. `close`
 * closes that tab on an attached browser and shuts a launched one.
 */
export async function openExtensionSession({ connect, headless, extensionDir, url, onError }) {
  const session = await openBrowser({ connect, headless, enableExtensions: true });
  const { browser } = session;
  let page = null;

  const close = async () => {
    if (session.isConnected && page && !page.isClosed()) await page.close();
    await session.close();
  };

  try {
    const extension = await installExtension({ browser, extensionDir });
    // An attached browser may be someone's own window, so leave its tabs alone.
    page = session.isConnected
      ? await browser.newPage()
      : ((await browser.pages())[0] ?? (await browser.newPage()));
    const errors = collectConsoleErrors({
      browser,
      page,
      targetUrlPrefix: extensionUrlPrefix(extension),
      onError,
    });
    console.log(`Opening ${url}`);
    await page.goto(url);
    return { browser, isConnected: session.isConnected, extension, page, errors, close };
  } catch (error) {
    await close();
    throw error;
  }
}
