// Where a launched browser records its DevTools endpoint, so Puppeteer scripts can attach to it.
import { rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { CHROME_CACHE_DIR } from './chrome.mjs';

/** In the gitignored .chrome folder at the repo root, next to the browser it describes. */
export const ENDPOINT_FILE = join(CHROME_CACHE_DIR, 'devtools-endpoint.txt');

export async function recordEndpoint(endpoint) {
  await writeFile(ENDPOINT_FILE, `${endpoint}\n`);
}

export async function clearEndpoint() {
  await rm(ENDPOINT_FILE, { force: true });
}
