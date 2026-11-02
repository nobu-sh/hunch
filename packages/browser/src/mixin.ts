// Mixins: small project files that set default options and drive the browser once it's open.
import { dirname, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import type { Browser, Page } from 'puppeteer-core';
import type { LoadedExtension } from './extensions.ts';
import { type BrowserOptions, resolvePaths } from './options.ts';

export interface MixinContext {
  browser: Browser;
  /** The tab the URL opened in. */
  page: Page;
  /** In the order the options listed them. */
  extensions: LoadedExtension[];
  /** Prints a line to the terminal. */
  log: (line: string) => void;
  options: BrowserOptions;
  /** The browser's DevTools websocket endpoint. */
  endpoint: string;
  /** Console errors from the page and the extensions, collected as they arrive. */
  errors: string[];
  /** Aborts when the browser closes or `seconds` run out: stop then. */
  signal: AbortSignal;
  /** The run's time limit, or null when it lasts until the browser closes. */
  seconds: number | null;
}

export interface Mixin {
  /** Defaults for this mixin; the command line overrides them. Paths resolve from the mixin file. */
  options?: Partial<BrowserOptions>;
  /**
   * Runs once the URL is open. Return an exit code to finish when it settles; return nothing to
   * stay open until the browser closes or `seconds` run out.
   */
  run?: (context: MixinContext) => Promise<number | undefined> | number | undefined;
}

/** Types a mixin. Export the result as the module's default. */
export function defineMixin(mixin: Mixin): Mixin {
  return mixin;
}

/** Imports the mixin at `path`, with its option paths anchored at its own folder. */
export async function loadMixin(path: string): Promise<Mixin> {
  const fullPath = resolve(path);
  const module = (await import(pathToFileURL(fullPath).href)) as { default?: unknown };
  const mixin = module.default;
  if (!isMixin(mixin)) {
    throw new Error(
      `${fullPath} must export a mixin as its default: export default defineMixin({...})`,
    );
  }
  const options = resolvePaths({ options: mixin.options ?? {}, baseDir: dirname(fullPath) });
  return { ...mixin, options };
}

function isMixin(value: unknown): value is Mixin {
  if (typeof value !== 'object' || value === null) return false;
  const { run } = value as { run?: unknown };
  return run === undefined || typeof run === 'function';
}
