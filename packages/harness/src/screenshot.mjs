import { mkdir } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';

const DEFAULT_MARGIN_PX = 16;

/** Saves a screenshot of the page's viewport to `path`, creating its folder if needed. */
export async function saveScreenshot({ page, path }) {
  const fullPath = await prepare(path);
  await page.screenshot({ path: fullPath });
  console.log(`Screenshot: ${path}`);
}

/**
 * Saves a screenshot of the first element matching `selector`, plus `margin` pixels around it,
 * kept inside the viewport.
 */
export async function saveElementScreenshot({ page, selector, path, margin = DEFAULT_MARGIN_PX }) {
  const element = await page.$(selector);
  const box = await element?.boundingBox();
  if (!box) throw new Error(`Can't screenshot ${selector}: it isn't on the page or isn't visible`);
  const viewport = await page.evaluate(() => ({
    width: document.documentElement.clientWidth,
    height: window.innerHeight,
  }));
  const fullPath = await prepare(path);
  await page.screenshot({ path: fullPath, clip: paddedClip({ box, margin, viewport }) });
  console.log(`Screenshot: ${path}`);
}

/** `box` grown by `margin` on every side, then cut to the viewport. */
export function paddedClip({ box, margin, viewport }) {
  const left = Math.max(0, box.x - margin);
  const top = Math.max(0, box.y - margin);
  const right = Math.min(viewport.width, box.x + box.width + margin);
  const bottom = Math.min(viewport.height, box.y + box.height + margin);
  return { x: left, y: top, width: right - left, height: bottom - top };
}

async function prepare(path) {
  const fullPath = resolve(path);
  await mkdir(dirname(fullPath), { recursive: true });
  return fullPath;
}
