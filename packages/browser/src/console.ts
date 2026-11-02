// Console messages from the page and from extension pages and workers.
import {
  type Browser,
  type ConsoleMessage,
  type Page,
  type Target,
  TargetType,
} from 'puppeteer-core';
import type { ConsoleMode } from './options.ts';

export interface ConsoleWatch {
  /** Every error so far, as `<source>: <text>`. */
  errors: string[];
  /** Stops printing; errors are still collected. */
  silence: () => void;
}

/**
 * Collects console errors from `page` and from every target whose URL starts with one of
 * `targetUrlPrefixes`, and prints messages as `mode` says.
 */
export function watchConsole(params: {
  browser: Browser;
  page: Page;
  targetUrlPrefixes: string[];
  mode: ConsoleMode;
  log: (line: string) => void;
}): ConsoleWatch {
  const { browser, page, targetUrlPrefixes, mode, log } = params;
  const errors: string[] = [];
  let isSilenced = false;

  const report = (params: { source: string; type: string; text: string }) => {
    const isError = params.type === 'error';
    if (isError) errors.push(`${params.source}: ${params.text}`);
    const isShown = mode === 'all' || (mode === 'errors' && isError);
    if (isShown && !isSilenced) log(`console ${params.type}  ${params.source}: ${params.text}`);
  };
  const onMessage = (source: string) => (message: ConsoleMessage) => {
    report({ source, type: message.type(), text: message.text() });
  };

  page.on('console', onMessage('page'));
  page.on('pageerror', (error) => {
    report({ source: 'page', type: 'error', text: String(error) });
  });

  const watchTarget = async (target: Target) => {
    const url = target.url();
    if (!targetUrlPrefixes.some((prefix) => url.startsWith(prefix))) return;
    const source = new URL(url).pathname.slice(1);
    if (target.type() === TargetType.SERVICE_WORKER) {
      (await target.worker())?.on('console', onMessage(source));
      return;
    }
    const targetPage = await target.asPage().catch(() => null);
    targetPage?.on('console', onMessage(source));
  };
  for (const target of browser.targets()) void watchTarget(target);
  browser.on('targetcreated', (target: Target) => void watchTarget(target));

  return {
    errors,
    silence: () => {
      isSilenced = true;
    },
  };
}
