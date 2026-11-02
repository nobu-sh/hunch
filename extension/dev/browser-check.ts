// `pnpm browser:check`: a headless plumbing check that the test beat's audio reaches Hunch's
// analyzer, read from the extension's own session state. Exits 1 if it doesn't.
import { defineMixin } from '@hunch/browser';
import { checkTestBeatSession } from './session.ts';
import { BUILD_DIR, followSession, turnOnHunch } from './setup.ts';
import { startTestBeat } from './test-beat.ts';

const LISTEN_SECONDS = 12;

const testBeat = await startTestBeat();

export default defineMixin({
  options: {
    extensions: [BUILD_DIR],
    url: testBeat.url,
    headless: true,
    seconds: LISTEN_SECONDS,
  },
  async run(context) {
    try {
      if (context.page.url() !== testBeat.url) {
        context.log(`FAIL  the check only works on the test beat; drop --url`);
        return 1;
      }
      const readState = await turnOnHunch({ context, testBeat });
      const state = await followSession({ context, readState });
      const problems = checkTestBeatSession({ state, errors: context.errors });
      for (const problem of problems) context.log(`FAIL  ${problem}`);
      if (problems.length === 0) context.log('PASS  test beat reached the analyzer');
      return problems.length === 0 ? 0 : 1;
    } finally {
      await testBeat.close();
    }
  },
});
