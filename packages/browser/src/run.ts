// One browser run: launch or attach, load extensions, open the URL, run the mixin, wait.
import { watchConsole } from './console.ts';
import { removeEndpoint, writeEndpoint } from './endpoint.ts';
import { type LoadedExtension, loadExtension } from './extensions.ts';
import { openBrowser } from './launch.ts';
import type { Mixin, MixinContext } from './mixin.ts';
import type { BrowserOptions } from './options.ts';

const MS_PER_SECOND = 1000;

/** Runs `mixin` (or nothing) in a browser opened with `options`, resolving to the exit code. */
export async function runBrowser(params: {
  options: BrowserOptions;
  mixin: Mixin | null;
  log: (line: string) => void;
}): Promise<number> {
  const { options, mixin, log } = params;
  const opened = await openBrowser(options);
  const { browser } = opened;
  const stop = new AbortController();
  browser.once('disconnected', () => {
    stop.abort();
  });
  const timer =
    options.seconds === null
      ? null
      : setTimeout(() => {
          stop.abort();
        }, options.seconds * MS_PER_SECOND);
  const hasEndpointFile = !opened.isConnected;
  let silenceConsole = () => {};

  try {
    const endpoint = browser.wsEndpoint();
    log(`DevTools: ${endpoint}`);
    if (hasEndpointFile) await writeEndpoint({ file: options.endpointFile, endpoint });

    const extensions: LoadedExtension[] = [];
    for (const path of options.extensions) {
      const extension = await loadExtension({ browser, path });
      log(`Extension ${extension.id}: ${path}`);
      extensions.push(extension);
    }
    // An attached browser may be someone's own window, so leave its tabs alone.
    const page = opened.isConnected
      ? await browser.newPage()
      : ((await browser.pages())[0] ?? (await browser.newPage()));
    const consoleWatch = watchConsole({
      browser,
      page,
      targetUrlPrefixes: extensions.map((extension) => extension.urlPrefix),
      mode: options.console,
      log,
    });
    silenceConsole = consoleWatch.silence;
    if (options.url !== null) {
      log(`Opening ${options.url}`);
      await page.goto(options.url);
    }

    const context: MixinContext = {
      browser,
      page,
      extensions,
      log,
      options,
      endpoint,
      errors: consoleWatch.errors,
      signal: stop.signal,
      seconds: options.seconds,
    };
    const exitCode = await mixin?.run?.(context);
    if (typeof exitCode === 'number') return exitCode;

    if (!stop.signal.aborted) log('Open. Close the browser to exit.');
    await aborted(stop.signal);
    return 0;
  } finally {
    if (timer) clearTimeout(timer);
    // Closing makes pages and extensions log errors about their own teardown; they aren't news.
    silenceConsole();
    if (hasEndpointFile) await removeEndpoint(options.endpointFile);
    await opened.close();
  }
}

function aborted(signal: AbortSignal): Promise<void> {
  if (signal.aborted) return Promise.resolve();
  return new Promise((resolve) => {
    signal.addEventListener('abort', () => {
      resolve();
    });
  });
}
