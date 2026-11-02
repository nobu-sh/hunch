// Chrome for Testing, kept in extension/.chrome. Branded Chrome ignores unpacked extensions
// passed on the command line, so the harness always uses this build.
import {
  Browser,
  detectBrowserPlatform,
  getInstalledBrowsers,
  install,
  resolveBuildId,
} from '@puppeteer/browsers';
import { fileURLToPath } from 'node:url';

export const CHROME_CACHE_DIR = fileURLToPath(new URL('../../.chrome', import.meta.url));
const CHANNEL = 'stable';

export async function installChrome() {
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
export async function chromeExecutable() {
  const installed = (await getInstalledBrowsers({ cacheDir: CHROME_CACHE_DIR }))
    .filter((entry) => entry.browser === Browser.CHROME)
    .sort((a, b) => b.buildId.localeCompare(a.buildId, undefined, { numeric: true }));
  const newest = installed[0];
  if (!newest) {
    throw new Error('Chrome for Testing is not installed. Run: pnpm browser:install');
  }
  return newest.executablePath;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  await installChrome();
}
