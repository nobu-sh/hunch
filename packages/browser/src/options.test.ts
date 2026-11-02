import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import {
  DEFAULT_OPTIONS,
  mergeOptions,
  parseCommandLine,
  parseWindowSize,
  resolvePaths,
} from './options.ts';

describe('parseCommandLine', () => {
  it('leaves out every flag that was not given', () => {
    expect(parseCommandLine([])).toEqual({
      options: {},
      mixin: null,
      isInstall: false,
      isHelp: false,
    });
  });

  it('parses launch and setup flags', () => {
    const { options } = parseCommandLine([
      '--url',
      'https://example.com',
      '--headless',
      '--extension',
      'a',
      '--extension',
      'b',
      '--seconds',
      '2.5',
      '--window-size',
      '1024x768',
      '--console',
      'all',
      '--chrome-arg=--lang=fr',
      '--mute',
    ]);
    expect(options).toEqual({
      url: 'https://example.com',
      headless: true,
      extensions: ['a', 'b'],
      seconds: 2.5,
      windowSize: { width: 1024, height: 768 },
      console: 'all',
      chromeArgs: ['--lang=fr'],
      mute: true,
    });
  });

  it('takes a mixin path', () => {
    expect(parseCommandLine(['--mixin', 'extension/dev/browser.ts']).mixin).toBe(
      'extension/dev/browser.ts',
    );
  });

  it('accepts install as a subcommand or a flag', () => {
    expect(parseCommandLine(['install']).isInstall).toBe(true);
    expect(parseCommandLine(['--install']).isInstall).toBe(true);
  });

  it('rejects stray arguments and bad values', () => {
    expect(() => parseCommandLine(['open'])).toThrow('Unexpected argument "open"');
    expect(() => parseCommandLine(['--seconds', '0'])).toThrow('got "0"');
    expect(() => parseCommandLine(['--console', 'warnings'])).toThrow('got "warnings"');
    expect(() => parseCommandLine(['--headed'])).toThrow(/--headed/);
  });
});

describe('parseWindowSize', () => {
  it('reads <w>x<h>', () => {
    expect(parseWindowSize('1280x800')).toEqual({ width: 1280, height: 800 });
  });

  it('rejects other shapes and zero sizes', () => {
    expect(() => parseWindowSize('1280,800')).toThrow('got "1280,800"');
    expect(() => parseWindowSize('0x800')).toThrow('got "0x800"');
  });
});

describe('resolvePaths', () => {
  it('anchors extension, profile and endpoint paths', () => {
    const base = resolve('/work/mixins');
    expect(
      resolvePaths({
        options: {
          extensions: ['../build'],
          profile: 'profile',
          endpointFile: 'endpoint.txt',
          url: 'x',
        },
        baseDir: base,
      }),
    ).toEqual({
      extensions: [resolve(base, '../build')],
      profile: resolve(base, 'profile'),
      endpointFile: resolve(base, 'endpoint.txt'),
      url: 'x',
    });
  });
});

describe('mergeOptions', () => {
  it('is headed with no URL by default', () => {
    expect(mergeOptions({ mixin: {}, commandLine: {} })).toEqual(DEFAULT_OPTIONS);
    expect(DEFAULT_OPTIONS.headless).toBe(false);
  });

  it('lets the command line override the mixin', () => {
    const merged = mergeOptions({
      mixin: { headless: true, seconds: 12, url: 'http://a/' },
      commandLine: { headless: false, url: 'http://b/' },
    });
    expect(merged).toMatchObject({ headless: false, seconds: 12, url: 'http://b/' });
  });

  it('ignores the mixin launch-only defaults with --connect', () => {
    const merged = mergeOptions({
      mixin: { headless: true, mute: true, windowSize: { width: 800, height: 600 }, seconds: 12 },
      commandLine: { connect: '9222' },
    });
    expect(merged).toMatchObject({
      connect: '9222',
      headless: false,
      mute: false,
      windowSize: null,
      seconds: 12,
    });
  });

  it('rejects a launch-only flag typed with --connect', () => {
    expect(() =>
      mergeOptions({ mixin: {}, commandLine: { connect: '9222', headless: true } }),
    ).toThrow('--headless only applies to a launched browser; drop it when using --connect');
  });

  it('names every launch-only flag typed with --connect', () => {
    expect(() =>
      mergeOptions({
        mixin: {},
        commandLine: { connect: '9222', headless: true, mute: true, chromeArgs: ['--x'] },
      }),
    ).toThrow(
      '--headless, --mute and --chrome-arg only apply to a launched browser; drop them when using --connect',
    );
  });

  it('allows --connect with extensions and a URL', () => {
    const merged = mergeOptions({
      mixin: { extensions: ['/x'] },
      commandLine: { connect: '9222', url: 'http://a/' },
    });
    expect(merged.connect).toBe('9222');
  });

  it('rejects --devtools with --headless', () => {
    expect(() =>
      mergeOptions({ mixin: { headless: true }, commandLine: { devtools: true } }),
    ).toThrow('--devtools');
  });
});
