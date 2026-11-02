// Command-line options for `hunch-harness extension`.
import { parseArgs } from 'node:util';

export const DEFAULT_EXTENSION_DIR = '.output/chrome-mv3';
/** How long --check listens when --seconds isn't given. */
export const DEFAULT_CHECK_SECONDS = 12;

/**
 * Options for `hunch-harness extension`. `seconds` is null when the browser should stay open
 * until it's closed. Throws on values that can't work together.
 */
export function parseExtensionOptions(args) {
  const { values } = parseArgs({
    args,
    options: {
      url: { type: 'string' },
      seconds: { type: 'string' },
      headless: { type: 'boolean', default: false },
      check: { type: 'boolean', default: false },
      connect: { type: 'string' },
      extension: { type: 'string', default: DEFAULT_EXTENSION_DIR },
      help: { type: 'boolean', default: false },
    },
  });
  if (values.check && values.url) {
    throw new Error('--check only works with the built-in test page; drop --url');
  }
  if (values.connect && values.headless) {
    throw new Error('--headless only applies to a launched browser; drop it with --connect');
  }
  return { ...values, seconds: parseSeconds(values) };
}

function parseSeconds({ seconds, check }) {
  if (seconds === undefined) return check ? DEFAULT_CHECK_SECONDS : null;
  const number = Number(seconds);
  if (!Number.isFinite(number) || number <= 0) {
    throw new Error(`--seconds must be a positive number, got "${seconds}"`);
  }
  return number;
}
