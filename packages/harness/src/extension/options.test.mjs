import { describe, expect, it } from 'vitest';
import { DEFAULT_CHECK_SECONDS, DEFAULT_EXTENSION_DIR, parseExtensionOptions } from './options.mjs';

describe('parseExtensionOptions', () => {
  it('is headed and stays open by default', () => {
    const options = parseExtensionOptions([]);
    expect(options.headless).toBe(false);
    expect(options.seconds).toBeNull();
    expect(options.extension).toBe(DEFAULT_EXTENSION_DIR);
  });

  it('runs headless with --headless', () => {
    expect(parseExtensionOptions(['--headless']).headless).toBe(true);
  });

  it('no longer accepts --headed or --keep-open', () => {
    expect(() => parseExtensionOptions(['--headed'])).toThrow(/--headed/);
    expect(() => parseExtensionOptions(['--keep-open'])).toThrow(/--keep-open/);
  });

  it('listens for a while before checking', () => {
    expect(parseExtensionOptions(['--check']).seconds).toBe(DEFAULT_CHECK_SECONDS);
    expect(parseExtensionOptions(['--check', '--seconds', '5']).seconds).toBe(5);
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
});
