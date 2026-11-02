import { describe, expect, it } from 'vitest';
import { describeProblems, findMismatches, versionFromTag } from './check.ts';

const AGREEING = [
  { path: 'package.json', version: '0.1.0' },
  { path: 'Cargo.toml', version: '0.1.0' },
];

describe('findMismatches', () => {
  it('finds nothing when every file agrees', () => {
    expect(findMismatches({ expected: '0.1.0', files: AGREEING })).toEqual([]);
  });

  it('lists each file with a different version', () => {
    const files = [...AGREEING, { path: 'extension/package.json', version: '0.0.0' }];
    expect(findMismatches({ expected: '0.1.0', files })).toEqual([
      { path: 'extension/package.json', version: '0.0.0' },
    ]);
  });
});

describe('versionFromTag', () => {
  it('reads a vX.Y.Z tag name or ref', () => {
    expect(versionFromTag('v0.2.0')).toBe('0.2.0');
    expect(versionFromTag('refs/tags/v0.2.0')).toBe('0.2.0');
  });

  it('returns null for anything else', () => {
    for (const ref of ['0.2.0', 'desktop-v0.2.0', 'v0.2', 'v0.2.0-rc.1', 'refs/heads/v0.2.0']) {
      expect(versionFromTag(ref), ref).toBeNull();
    }
  });
});

describe('describeProblems', () => {
  it('reports nothing when files and tag agree', () => {
    expect(describeProblems({ expected: '0.1.0', files: AGREEING, tag: 'v0.1.0' })).toEqual([]);
  });

  it('names each mismatched file with both versions', () => {
    const files = [...AGREEING, { path: 'Cargo.toml', version: '0.0.9' }];
    expect(describeProblems({ expected: '0.1.0', files })).toEqual([
      'Cargo.toml has 0.0.9, expected 0.1.0',
    ]);
  });

  it('reports a tag that names another version', () => {
    expect(describeProblems({ expected: '0.1.1', files: [], tag: 'v0.1.0' })).toEqual([
      'tag v0.1.0 names 0.1.0, but the version is 0.1.1',
    ]);
  });

  it('reports a tag that is not a version tag', () => {
    expect(describeProblems({ expected: '0.1.0', files: [], tag: 'desktop-v0.1.0' })).toEqual([
      'tag desktop-v0.1.0 is not a vMAJOR.MINOR.PATCH tag',
    ]);
  });

  it('reports a root version that is not MAJOR.MINOR.PATCH', () => {
    expect(describeProblems({ expected: '0.1', files: [] })).toEqual([
      'the root version 0.1 is not MAJOR.MINOR.PATCH',
    ]);
  });
});
