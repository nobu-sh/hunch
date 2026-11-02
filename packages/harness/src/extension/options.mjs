// Command-line options for the extension commands.
import { parseArgs } from 'node:util';

export const DEFAULT_EXTENSION_DIR = '.output/chrome-mv3';
export const DEFAULT_SCREENSHOTS_DIR = '.output/screenshots';
const DEFAULT_SECONDS = '12';

/** Options for `hunch-harness extension`. Throws on values that can't work together. */
export function parseExtensionOptions(args) {
  const { values } = parseArgs({
    args,
    options: {
      url: { type: 'string' },
      seconds: { type: 'string', default: DEFAULT_SECONDS },
      headless: { type: 'boolean', default: false },
      'keep-open': { type: 'boolean', default: false },
      screenshot: { type: 'string' },
      check: { type: 'boolean', default: false },
      connect: { type: 'string' },
      extension: { type: 'string', default: DEFAULT_EXTENSION_DIR },
      help: { type: 'boolean', default: false },
    },
  });
  const seconds = Number(values.seconds);
  if (!Number.isFinite(seconds) || seconds <= 0) {
    throw new Error(`--seconds must be a positive number, got "${values.seconds}"`);
  }
  if (values.check && values.url) {
    throw new Error('--check only works with the built-in test page; drop --url');
  }
  if (values.connect && values.headless) {
    throw new Error('--headless only applies to a launched browser; drop it with --connect');
  }
  if (values.headless && values['keep-open']) {
    throw new Error('--keep-open needs a visible window; drop --headless');
  }
  return { ...values, seconds };
}

/** Options for `hunch-harness screenshots`. No `--out` means `DEFAULT_SCREENSHOTS_DIR`. */
export function parseScreenshotsOptions(args) {
  const { values } = parseArgs({
    args,
    options: {
      out: { type: 'string' },
      extension: { type: 'string', default: DEFAULT_EXTENSION_DIR },
      help: { type: 'boolean', default: false },
    },
  });
  return values;
}
