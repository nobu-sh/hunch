// `pnpm browser:desktop`: attaches to the running dev app's webview and prints its session from
// the dev state it publishes on `window.__hunchDev`: a readout line when it changes, and each
// window start, window end and source status as it arrives. Runs until the app closes.
import { defineMixin, type MixinContext } from '@hunch/browser';
import {
  DESKTOP_DEV_STATE_GLOBAL,
  type DesktopDevState,
  type DevLogEntry,
  formatLogEntry,
  formatSession,
} from '@hunch/ui/dev';
import { WEBVIEW2_DEBUG_PORT } from './debug-port.ts';

const POLL_MS = 500;
const MS_PER_SECOND = 1000;

/** Reads the app's dev state; `null` when the page doesn't publish one. */
async function readDevState(context: MixinContext): Promise<DesktopDevState | null> {
  try {
    return await context.page.evaluate((name) => {
      const published = (window as unknown as Record<string, DesktopDevState | undefined>)[name];
      return published ?? null;
    }, DESKTOP_DEV_STATE_GLOBAL);
  } catch {
    // The page reloads during development; the next poll reads it again.
    return null;
  }
}

/** Log entries worth a line of their own; estimates show up in the readout instead. */
function isMilestone(entry: DevLogEntry): boolean {
  return entry.kind === 'sourceStatus' || entry.event.type !== 'estimate';
}

function formatReadout(devState: DesktopDevState): string {
  return `source ${devState.source?.state ?? '—'} | ${formatSession(devState.state)}`;
}

/** Follows the log's sequence numbers to hand out each entry once, saying when some were missed. */
function createLogReader(): (log: DevLogEntry[]) => { fresh: DevLogEntry[]; missed: number } {
  let lastSeq = 0;
  return (log) => {
    const newestSeq = log.at(-1)?.seq ?? 0;
    // A reload restarts the numbering.
    if (newestSeq < lastSeq) lastSeq = 0;
    const fresh = log.filter((entry) => entry.seq > lastSeq);
    const oldestFreshSeq = fresh[0]?.seq ?? lastSeq + 1;
    const missed = lastSeq > 0 ? oldestFreshSeq - lastSeq - 1 : 0;
    lastSeq = Math.max(lastSeq, newestSeq);
    return { fresh, missed };
  };
}

export default defineMixin({
  options: { connect: String(WEBVIEW2_DEBUG_PORT), reusePage: true },
  async run(context) {
    const { page, log, signal } = context;
    log(`Attached to ${page.url()}`);
    const startedAt = Date.now();
    const logLine = (line: string) => {
      const elapsed = ((Date.now() - startedAt) / MS_PER_SECOND).toFixed(1);
      log(`${elapsed.padStart(5)}s  ${line}`);
    };
    const readLog = createLogReader();
    let lastReadout = '';
    while (!signal.aborted) {
      const devState = await readDevState(context);
      const readout = devState
        ? formatReadout(devState)
        : `no window.${DESKTOP_DEV_STATE_GLOBAL}: is this the app from pnpm desktop:dev?`;
      if (devState) {
        const { fresh, missed } = readLog(devState.log);
        if (missed > 0) logLine(`missed ${String(missed)} log entries`);
        for (const entry of fresh.filter(isMilestone)) logLine(formatLogEntry(entry));
      }
      if (readout !== lastReadout) {
        logLine(readout);
        lastReadout = readout;
      }
      await new Promise((resolve) => setTimeout(resolve, POLL_MS));
    }
  },
});
