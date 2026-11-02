// Loading unpacked extensions into a running browser.
import { readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { type Browser, type Extension, TargetType } from 'puppeteer-core';

export interface LoadedExtension {
  id: string;
  /** The unpacked folder it was loaded from. */
  path: string;
  /** Puppeteer's handle: `triggerAction(page)` clicks its toolbar icon. */
  handle: Extension;
  /** `chrome-extension://<id>/`, which all its pages and workers share. */
  urlPrefix: string;
}

interface Manifest {
  background?: { service_worker?: string };
}

/**
 * Installs the unpacked extension at `path` and, when it has one, waits until its service worker
 * runs: a toolbar click that lands earlier is lost.
 */
export async function loadExtension(params: {
  browser: Browser;
  path: string;
}): Promise<LoadedExtension> {
  const { browser, path } = params;
  const manifest = await readManifest(path);
  // Installed after launch: passing the path to `launch` doesn't finish loading it.
  const id = await browser.installExtension(path).catch((error: unknown) => {
    throw new Error(
      `Couldn't install the extension at ${path}. An attached browser needs ` +
        '--enable-unsafe-extension-debugging, and embedded webviews may not install any.',
      { cause: error },
    );
  });
  const handle = (await browser.extensions()).get(id);
  if (!handle) throw new Error(`Chrome didn't list the extension it installed from ${path}`);
  const urlPrefix = `chrome-extension://${id}/`;
  if (manifest.background?.service_worker) {
    await browser.waitForTarget(
      (target) => target.type() === TargetType.SERVICE_WORKER && target.url().startsWith(urlPrefix),
    );
  }
  return { id, path, handle, urlPrefix };
}

async function readManifest(path: string): Promise<Manifest> {
  const manifestPath = join(path, 'manifest.json');
  try {
    return JSON.parse(await readFile(manifestPath, 'utf8')) as Manifest;
  } catch (error) {
    throw new Error(`No unpacked extension at ${path}: can't read ${manifestPath}`, {
      cause: error,
    });
  }
}
