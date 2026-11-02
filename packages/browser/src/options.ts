// Browser options: their defaults, the command line, and how a mixin's defaults combine with them.
import { join, resolve } from 'node:path';
import { parseArgs } from 'node:util';
import { CHROME_CACHE_DIR } from './chrome.ts';

export const CONSOLE_MODES = ['errors', 'all', 'none'] as const;
export type ConsoleMode = (typeof CONSOLE_MODES)[number];

export interface WindowSize {
  width: number;
  height: number;
}

export interface BrowserOptions {
  /** Page to open; null leaves the first tab blank. */
  url: string | null;
  headless: boolean;
  /** Attach to this browser instead of launching one: a port, http(s) URL or ws(s) endpoint. */
  connect: string | null;
  /** Unpacked extension folders to load. */
  extensions: string[];
  /** Close after this long; null stays open until the browser closes. */
  seconds: number | null;
  windowSize: WindowSize | null;
  /** Open DevTools for the page. */
  devtools: boolean;
  /** Persistent user data folder; null uses a fresh temporary profile. */
  profile: string | null;
  /** Mute the browser. Off by default because muting Chrome also silences tab capture. */
  mute: boolean;
  /** Which console messages to print: errors are always collected either way. */
  console: ConsoleMode;
  /** Where a launched browser writes its DevTools endpoint while it runs. */
  endpointFile: string;
  /** Extra Chrome command-line flags. */
  chromeArgs: string[];
}

/** What the command line asked for, beyond the browser options themselves. */
export interface CommandLine {
  options: Partial<BrowserOptions>;
  mixin: string | null;
  isInstall: boolean;
  isHelp: boolean;
}

export const DEFAULT_OPTIONS: BrowserOptions = {
  url: null,
  headless: false,
  connect: null,
  extensions: [],
  seconds: null,
  windowSize: null,
  devtools: false,
  profile: null,
  mute: false,
  console: 'errors',
  endpointFile: join(CHROME_CACHE_DIR, 'devtools-endpoint.txt'),
  chromeArgs: [],
};

/** Options that only mean something for a browser this tool launches, with their flags. */
const LAUNCH_ONLY_FLAGS = {
  headless: '--headless',
  windowSize: '--window-size',
  devtools: '--devtools',
  profile: '--profile',
  mute: '--mute',
  chromeArgs: '--chrome-arg',
} as const satisfies Partial<Record<keyof BrowserOptions, string>>;
type LaunchOnlyOption = keyof typeof LAUNCH_ONLY_FLAGS;
const LAUNCH_ONLY = Object.keys(LAUNCH_ONLY_FLAGS) as LaunchOnlyOption[];
const INSTALL_COMMAND = 'install';
const WINDOW_SIZE_PATTERN = /^(\d+)x(\d+)$/;

/** Parses `args`. Paths stay as typed; `resolvePaths` anchors them. Throws on bad values. */
export function parseCommandLine(args: string[]): CommandLine {
  const { values, positionals } = parseArgs({
    args,
    allowPositionals: true,
    options: {
      url: { type: 'string' },
      headless: { type: 'boolean' },
      connect: { type: 'string' },
      extension: { type: 'string', multiple: true },
      seconds: { type: 'string' },
      'window-size': { type: 'string' },
      devtools: { type: 'boolean' },
      profile: { type: 'string' },
      mute: { type: 'boolean' },
      console: { type: 'string' },
      'endpoint-file': { type: 'string' },
      'chrome-arg': { type: 'string', multiple: true },
      mixin: { type: 'string' },
      install: { type: 'boolean' },
      help: { type: 'boolean' },
    },
  });
  const [command, ...extra] = positionals;
  const isInstallCommand = command === INSTALL_COMMAND;
  const unexpected = isInstallCommand ? extra : positionals;
  if (unexpected.length > 0) {
    throw new Error(`Unexpected argument "${unexpected.join(' ')}". See --help.`);
  }

  const options: Partial<BrowserOptions> = {
    url: values.url,
    headless: values.headless,
    connect: values.connect,
    extensions: values.extension,
    seconds: values.seconds === undefined ? undefined : parseSeconds(values.seconds),
    windowSize:
      values['window-size'] === undefined ? undefined : parseWindowSize(values['window-size']),
    devtools: values.devtools,
    profile: values.profile,
    mute: values.mute,
    console: values.console === undefined ? undefined : parseConsoleMode(values.console),
    endpointFile: values['endpoint-file'],
    chromeArgs: values['chrome-arg'],
  };
  return {
    options: withoutUndefined(options),
    mixin: values.mixin ?? null,
    isInstall: isInstallCommand || values.install === true,
    isHelp: values.help === true,
  };
}

export function parseSeconds(value: string): number {
  const seconds = Number(value);
  if (!Number.isFinite(seconds) || seconds <= 0) {
    throw new Error(`--seconds must be a positive number, got "${value}"`);
  }
  return seconds;
}

export function parseWindowSize(value: string): WindowSize {
  const match = WINDOW_SIZE_PATTERN.exec(value);
  const width = Number(match?.[1]);
  const height = Number(match?.[2]);
  if (!match || width === 0 || height === 0) {
    throw new Error(`--window-size must look like 1280x800, got "${value}"`);
  }
  return { width, height };
}

function parseConsoleMode(value: string): ConsoleMode {
  const mode = CONSOLE_MODES.find((candidate) => candidate === value);
  if (!mode) throw new Error(`--console must be ${CONSOLE_MODES.join(', ')}, got "${value}"`);
  return mode;
}

function withoutUndefined<T extends object>(value: T): Partial<T> {
  return Object.fromEntries(
    Object.entries(value).filter(([, entry]) => entry !== undefined),
  ) as Partial<T>;
}

/** Anchors the relative paths in `options` (extensions, profile, endpoint file) at `baseDir`. */
export function resolvePaths(params: {
  options: Partial<BrowserOptions>;
  baseDir: string;
}): Partial<BrowserOptions> {
  const { options, baseDir } = params;
  const anchored: Partial<BrowserOptions> = { ...options };
  if (options.extensions)
    anchored.extensions = options.extensions.map((dir) => resolve(baseDir, dir));
  if (options.profile) anchored.profile = resolve(baseDir, options.profile);
  if (options.endpointFile) anchored.endpointFile = resolve(baseDir, options.endpointFile);
  return anchored;
}

/**
 * The options to run with: defaults, then the mixin's, then the command line's, each replacing
 * the one before. With --connect, the mixin's launch-only defaults are dropped and only launch-only
 * flags typed on the command line are an error. Throws on combinations that can't work.
 */
export function mergeOptions(params: {
  mixin: Partial<BrowserOptions>;
  commandLine: Partial<BrowserOptions>;
}): BrowserOptions {
  const { commandLine } = params;
  const connect = commandLine.connect ?? params.mixin.connect ?? null;
  const isConnecting = connect !== null;
  if (isConnecting) {
    const typed = LAUNCH_ONLY.filter((key) => commandLine[key] !== undefined);
    if (typed.length > 0) throw new Error(launchOnlyConflictMessage(typed));
  }
  const mixin = isConnecting ? withoutLaunchOnly(params.mixin) : params.mixin;
  const merged: BrowserOptions = { ...DEFAULT_OPTIONS, ...mixin, ...commandLine };
  if (merged.headless && merged.devtools) {
    throw new Error('--devtools needs a visible window; drop --headless');
  }
  return merged;
}

function withoutLaunchOnly(options: Partial<BrowserOptions>): Partial<BrowserOptions> {
  const isLaunchOnly = (key: string) => key in LAUNCH_ONLY_FLAGS;
  return Object.fromEntries(Object.entries(options).filter(([key]) => !isLaunchOnly(key)));
}

function launchOnlyConflictMessage(options: LaunchOnlyOption[]): string {
  const flags: string[] = options.map((key) => LAUNCH_ONLY_FLAGS[key]);
  const last = flags.pop() ?? '';
  const isOne = flags.length === 0;
  const listed = isOne ? last : `${flags.join(', ')} and ${last}`;
  const verb = isOne ? 'applies' : 'apply';
  const pronoun = isOne ? 'it' : 'them';
  return `${listed} only ${verb} to a launched browser; drop ${pronoun} when using --connect`;
}
