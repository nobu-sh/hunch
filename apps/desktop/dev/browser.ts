// `pnpm browser:desktop`: attaches to the running dev app's webview and prints the overlay's key
// and BPM as they change, until the app closes.
import { defineMixin } from '@hunch/browser';
import { WEBVIEW2_DEBUG_PORT } from './debug-port.ts';

const POLL_MS = 1000;

export default defineMixin({
  options: { connect: String(WEBVIEW2_DEBUG_PORT), reusePage: true },
  async run({ page, log, signal }) {
    log(`Attached to ${page.url()}`);
    let last = '';
    while (!signal.aborted) {
      const readout = await page
        .evaluate(() => {
          const text = (ref: string) =>
            document.querySelector(`[data-ref="${ref}"]`)?.textContent ?? '';
          return `${text('status')} | key ${text('key')} | ${text('bpm')} BPM`;
        })
        .catch(() => null);
      if (readout !== null && readout !== last) {
        log(readout);
        last = readout;
      }
      await new Promise((resolve) => setTimeout(resolve, POLL_MS));
    }
  },
});
