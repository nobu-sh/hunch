#!/usr/bin/env node
// `pnpm release:version` and `pnpm release:check`. Node runs this file directly by stripping its types.
import { execFileSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { parseArgs } from 'node:util';
import { describeProblems, type FileVersion } from './check.ts';
import {
  isValidVersion,
  readVersion,
  SOURCE_FILE,
  VERSION_FILES,
  writeVersion,
  type VersionFile,
} from './versions.ts';

const REPO_ROOT = resolve(import.meta.dirname, '../../..');

const HELP = `Usage: release version <x.y.z>
       release check [--tag <ref>]

version  Writes x.y.z to every file that carries the product version, then refreshes Cargo.lock.
check    Exits 1, listing the mismatches, unless every file has the root package.json's version.
         With --tag, a vX.Y.Z tag (or refs/tags/vX.Y.Z) must name that version too.

Files: ${VERSION_FILES.map((file) => file.path).join(', ')}`;

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function readFileVersion(file: VersionFile): FileVersion {
  const text = readFileSync(resolve(REPO_ROOT, file.path), 'utf8');
  try {
    return { path: file.path, version: readVersion({ text, format: file.format }) };
  } catch (error) {
    throw new Error(`${file.path}: ${errorMessage(error)}`, { cause: error });
  }
}

function setVersion(version: string): number {
  if (!isValidVersion(version)) {
    console.error(`release version: ${version} is not MAJOR.MINOR.PATCH, e.g. 0.2.0`);
    return 1;
  }
  for (const file of VERSION_FILES) {
    const path = resolve(REPO_ROOT, file.path);
    const text = readFileSync(path, 'utf8');
    try {
      writeFileSync(path, writeVersion({ text, format: file.format, version }));
    } catch (error) {
      throw new Error(`${file.path}: ${errorMessage(error)}`, { cause: error });
    }
    console.log(`${file.path}: ${version}`);
  }
  // Only the workspace crates' own entries change, so nothing needs the network.
  execFileSync('cargo', ['update', '--workspace', '--offline'], {
    cwd: REPO_ROOT,
    stdio: 'inherit',
  });
  return 0;
}

function checkVersions(tag: string | undefined): number {
  const expected = readFileVersion(SOURCE_FILE).version;
  const files = VERSION_FILES.map(readFileVersion);
  const problems = describeProblems({ expected, files, tag });
  if (problems.length > 0) {
    console.error(`release check failed:\n${problems.map((line) => `  ${line}`).join('\n')}`);
    console.error('Run `pnpm release:version <x.y.z>` to set every file to one version.');
    return 1;
  }
  const tagNote = tag === undefined ? '' : ` and tag ${tag}`;
  console.log(`release check: ${String(files.length)} files${tagNote} agree on ${expected}`);
  return 0;
}

function main(args: string[]): number {
  const { values, positionals } = parseArgs({
    args,
    allowPositionals: true,
    options: { tag: { type: 'string' }, help: { type: 'boolean' } },
  });
  const [command, ...rest] = positionals;
  if (values.help) {
    console.log(HELP);
    return 0;
  }
  if (command === 'version' && rest.length === 1 && rest[0] !== undefined) {
    return setVersion(rest[0]);
  }
  if (command === 'check' && rest.length === 0) {
    return checkVersions(values.tag);
  }
  console.error(HELP);
  return 1;
}

try {
  process.exitCode = main(process.argv.slice(2));
} catch (error) {
  console.error(`release: ${errorMessage(error)}`);
  process.exitCode = 1;
}
