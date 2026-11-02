// Comparing the versions in every file, and a release tag, against the source of truth.

import { isValidVersion } from './versions.ts';

export interface FileVersion {
  path: string;
  version: string;
}

/** The files whose version differs from `expected`. */
export function findMismatches({
  expected,
  files,
}: {
  expected: string;
  files: FileVersion[];
}): FileVersion[] {
  return files.filter((file) => file.version !== expected);
}

const TAG_REF_PREFIX = 'refs/tags/';
const TAG_PREFIX = 'v';

/** The version a `vX.Y.Z` tag names, from a tag name or a `refs/tags/` ref; null if it isn't one. */
export function versionFromTag(ref: string): string | null {
  const tag = ref.startsWith(TAG_REF_PREFIX) ? ref.slice(TAG_REF_PREFIX.length) : ref;
  if (!tag.startsWith(TAG_PREFIX)) return null;
  const version = tag.slice(TAG_PREFIX.length);
  return isValidVersion(version) ? version : null;
}

/** Every problem with the versions and the optional tag, as lines to print; empty when they agree. */
export function describeProblems({
  expected,
  files,
  tag,
}: {
  expected: string;
  files: FileVersion[];
  tag?: string;
}): string[] {
  const problems: string[] = [];
  if (!isValidVersion(expected)) {
    problems.push(`the root version ${expected} is not MAJOR.MINOR.PATCH`);
  }
  for (const file of findMismatches({ expected, files })) {
    problems.push(`${file.path} has ${file.version}, expected ${expected}`);
  }
  if (tag !== undefined) {
    const tagVersion = versionFromTag(tag);
    if (tagVersion === null) {
      problems.push(`tag ${tag} is not a vMAJOR.MINOR.PATCH tag`);
    } else if (tagVersion !== expected) {
      problems.push(`tag ${tag} names ${tagVersion}, but the version is ${expected}`);
    }
  }
  return problems;
}
