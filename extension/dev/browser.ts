// `pnpm browser:extension`: the built extension on the test beat, turned on, with the tab's
// session state printed as it changes until the browser closes.
import { defineMixin } from '@hunch/browser';
import { BUILD_DIR, followSession, turnOnHunch } from './setup.ts';
import { startTestBeat } from './test-beat.ts';

const testBeat = await startTestBeat();

export default defineMixin({
  options: { extensions: [BUILD_DIR], url: testBeat.url },
  async run(context) {
    try {
      const readState = await turnOnHunch({ context, testBeat });
      await followSession({ context, readState });
    } finally {
      await testBeat.close();
    }
  },
});
