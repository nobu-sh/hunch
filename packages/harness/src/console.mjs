/**
 * Collects console errors from the page and from every target whose URL starts with
 * `targetUrlPrefix`, such as an extension's service worker and offscreen document. The returned
 * array fills as errors arrive, and `onError` sees each one as it comes in.
 */
export function collectConsoleErrors({ browser, page, targetUrlPrefix, onError }) {
  const errors = [];
  const add = (error) => {
    errors.push(error);
    onError?.(error);
  };
  const record = (source) => (message) => {
    if (message.type() === 'error') add(`${source}: ${message.text()}`);
  };
  page.on('console', record('page'));
  page.on('pageerror', (error) => add(`page: ${error.message}`));
  if (!targetUrlPrefix) return errors;

  const watchTarget = async (target) => {
    if (!target.url().startsWith(targetUrlPrefix)) return;
    const name = new URL(target.url()).pathname.slice(1);
    if (target.type() === 'service_worker') {
      (await target.worker())?.on('console', record(name));
      return;
    }
    const targetPage = await target.asPage().catch(() => null);
    targetPage?.on('console', record(name));
  };
  for (const target of browser.targets()) void watchTarget(target);
  browser.on('targetcreated', (target) => void watchTarget(target));
  return errors;
}
