import { describe, expect, it } from 'vitest';
import {
  DEFAULT_EXTENSION_DIR,
  parseExtensionOptions,
  parseScreenshotsOptions,
} from './options.mjs';

describe('parseExtensionOptions', () => {
  it('is headed by default', () => {
    const options = parseExtensionOptions([]);
    expect(options.headless).toBe(false);
    expect(options.seconds).toBe(12);
    expect(options.extension).toBe(DEFAULT_EXTENSION_DIR);
  });

  it('runs headless with --headless', () => {
    expect(parseExtensionOptions(['--headless', '--check']).headless).toBe(true);
  });

  it('no longer accepts --headed', () => {
    expect(() => parseExtensionOptions(['--headed'])).toThrow(/--headed/);
  });

  it('parses --seconds as a number', () => {
    expect(parseExtensionOptions(['--seconds', '2.5']).seconds).toBe(2.5);
  });

  it('rejects a seconds value that is not positive', () => {
    expect(() => parseExtensionOptions(['--seconds', '0'])).toThrow('got "0"');
    expect(() => parseExtensionOptions(['--seconds', 'soon'])).toThrow('got "soon"');
  });

  it('rejects --check with --url', () => {
    expect(() => parseExtensionOptions(['--check', '--url', 'https://example.com'])).toThrow(
      /--check/,
    );
  });

  it('rejects --headless with --connect', () => {
    expect(() => parseExtensionOptions(['--headless', '--connect', '9222'])).toThrow(/--connect/);
  });

  it('rejects --keep-open with --headless', () => {
    expect(() => parseExtensionOptions(['--headless', '--keep-open'])).toThrow(/--keep-open/);
  });
});

describe('parseScreenshotsOptions', () => {
  it('defaults the build and leaves the output folder to the command', () => {
    expect(parseScreenshotsOptions([])).toEqual({
      extension: DEFAULT_EXTENSION_DIR,
      help: false,
    });
  });

  it('takes --out', () => {
    expect(parseScreenshotsOptions(['--out', 'shots']).out).toBe('shots');
  });
});
