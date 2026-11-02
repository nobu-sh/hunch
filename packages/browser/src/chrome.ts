// Chrome for Testing, kept in the repo root's .chrome so every package shares one download.
// Branded Chrome ignores unpacked extensions passed on the command line, so this always uses
// Chrome for Testing.
import {
  Browser,
  detectBrowserPlatform,
  getInstalledBrowsers,
  install,
  resolveBuildId,
} from '@puppeteer/browsers';
import { fileURLToPath } from 'node:url';

/** The workspace root's .chrome folder: this file sits in <root>/packages/<name>/src. */
export const CHROME_CACHE_DIR = fileURLToPath(new URL('../../../.chrome', import.meta.url));
const CHANNEL = 'stable';

export async function installChrome(): Promise<string> {
  const platform = detectBrowserPlatform();
  if (!platform) throw new Error(`Chrome for Testing has no build for ${process.platform}`);
  const buildId = await resolveBuildId(Browser.CHROME, platform, CHANNEL);
  console.log(`Installing Chrome for Testing ${buildId} into ${CHROME_CACHE_DIR}`);
  const installed = await install({
    browser: Browser.CHROME,
    buildId,
    cacheDir: CHROME_CACHE_DIR,
    downloadProgressCallback: 'default',
  });
  console.log(`Chrome for Testing ready: ${installed.executablePath}`);
  return installed.executablePath;
}

/** The newest installed Chrome for Testing, or a clear error saying how to get one. */
export async function chromeExecutable(): Promise<string> {
  const installed = (await getInstalledBrowsers({ cacheDir: CHROME_CACHE_DIR }))
    .filter((entry) => entry.browser === Browser.CHROME)
    .sort((a, b) => b.buildId.localeCompare(a.buildId, undefined, { numeric: true }));
  const newest = installed[0];
  if (!newest) {
    throw new Error('Chrome for Testing is not installed. Run: pnpm browser install');
  }
  return newest.executablePath;
}
