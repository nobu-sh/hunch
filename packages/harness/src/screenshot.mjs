import { mkdir } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';

/** Saves a screenshot of the page's viewport to `path`, creating its folder if needed. */
export async function saveScreenshot({ page, path }) {
  const fullPath = resolve(path);
  await mkdir(dirname(fullPath), { recursive: true });
  await page.screenshot({ path: fullPath });
  console.log(`Screenshot: ${path}`);
}
