import { describe, expect, it } from 'vitest';
import { isValidVersion, readVersion, writeVersion } from './versions.ts';

const PACKAGE_JSON = `{
  "name": "hunch",
  "version": "0.1.0",
  "scripts": {
    "lint": "eslint ."
  },
  "nested": { "version": "9.9.9" }
}
`;

const CARGO_TOML = `[workspace]
members = ["crates/hunch-core"]

[workspace.package]
edition = "2024"
version = "0.1.0" # shared
license = "MIT"

[workspace.dependencies]
serde = { version = "1" }
`;

describe('isValidVersion', () => {
  it('accepts MAJOR.MINOR.PATCH', () => {
    expect(isValidVersion('0.1.0')).toBe(true);
    expect(isValidVersion('12.0.65535')).toBe(true);
  });

  it('rejects suffixes, leading zeros, missing parts and parts Chrome refuses', () => {
    for (const version of ['0.1', '0.1.0-beta.1', '0.1.0+abc', '01.0.0', 'v0.1.0', '0.1.65536']) {
      expect(isValidVersion(version), version).toBe(false);
    }
  });
});

describe('readVersion', () => {
  it('reads the top-level JSON version, not a nested one', () => {
    expect(readVersion({ text: PACKAGE_JSON, format: 'json' })).toBe('0.1.0');
  });

  it('reads the version in [workspace.package], not a dependency version', () => {
    expect(readVersion({ text: CARGO_TOML, format: 'cargo-workspace' })).toBe('0.1.0');
  });

  it('throws when the version is missing', () => {
    expect(() => readVersion({ text: '{ "name": "x" }', format: 'json' })).toThrow(/version/);
    expect(() => readVersion({ text: '[workspace]\n', format: 'cargo-workspace' })).toThrow(
      /workspace\.package/,
    );
  });
});

describe('writeVersion', () => {
  it('changes only the top-level JSON version line', () => {
    const written = writeVersion({ text: PACKAGE_JSON, format: 'json', version: '0.2.0' });
    expect(written).toBe(PACKAGE_JSON.replace('"version": "0.1.0"', '"version": "0.2.0"'));
  });

  it('changes only the workspace version and keeps its comment', () => {
    const written = writeVersion({ text: CARGO_TOML, format: 'cargo-workspace', version: '1.0.0' });
    expect(written).toBe(CARGO_TOML.replace('version = "0.1.0"', 'version = "1.0.0"'));
  });

  it('writes LF line endings', () => {
    const crlf = CARGO_TOML.replace(/\n/g, '\r\n');
    const written = writeVersion({ text: crlf, format: 'cargo-workspace', version: '1.0.0' });
    expect(written).not.toContain('\r');
  });

  it('throws when there is no top-level version line to rewrite', () => {
    expect(() =>
      writeVersion({ text: '{\n  "name": "x"\n}\n', format: 'json', version: '1.0.0' }),
    ).toThrow(/version/);
  });
});
