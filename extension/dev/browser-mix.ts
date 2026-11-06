// `pnpm browser:mix`: the built extension on the test mix, headed, turned on, with the tab's
// session state printed as it changes. Once the time is up it prints why each window started and
// ended, and exits 1 unless the change split the old song's window from the new one's.
import { defineMixin } from '@hunch/browser';
import { checkTestMixSession, formatWindowEvent } from './session.ts';
import { BUILD_DIR, followSession, recordWindowEvents, turnOnHunch } from './setup.ts';
import { TEST_MIX_CHANGE_SECONDS, startTestMix } from './test-beat.ts';

/** Past the change, plus the detector's few seconds of delay, but short of the loop's next change. */
const LISTEN_SECONDS = TEST_MIX_CHANGE_SECONDS + 30;

const testMix = await startTestMix();

export default defineMixin({
  options: { extensions: [BUILD_DIR], url: testMix.url, seconds: LISTEN_SECONDS },
  async run(context) {
    try {
      const readState = await turnOnHunch({ context, testBeat: testMix });
      const readWindowEvents = await recordWindowEvents(context);
      context.log(`The songs change ${String(TEST_MIX_CHANGE_SECONDS)} s into the mix.`);
      const state = await followSession({ context, readState });
      // Empty when the browser was closed by hand rather than by the time running out.
      const windowEvents = await readWindowEvents().catch(() => []);
      for (const event of windowEvents) context.log(formatWindowEvent(event));
      context.log(`history: ${JSON.stringify(state?.history ?? [])}`);
      const problems = checkTestMixSession({ state, windowEvents, errors: context.errors });
      for (const problem of problems) context.log(`FAIL  ${problem}`);
      if (problems.length === 0) context.log('PASS  the test mix split at its tempo change');
      return problems.length === 0 ? 0 : 1;
    } finally {
      await testMix.close();
    }
  },
});
