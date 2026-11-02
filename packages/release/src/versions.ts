// Where the product version lives, and how to read and rewrite it without touching anything else.

export type VersionFormat = 'json' | 'cargo-workspace';

export interface VersionFile {
  /** Relative to the repo root. */
  path: string;
  format: VersionFormat;
}

/** The source of truth: every other file follows the root package.json. */
export const SOURCE_FILE: VersionFile = { path: 'package.json', format: 'json' };

/** Every file that carries the shipped version, the source first. */
export const VERSION_FILES: VersionFile[] = [
  SOURCE_FILE,
  // WXT writes this into the extension's manifest.
  { path: 'extension/package.json', format: 'json' },
  { path: 'apps/desktop/package.json', format: 'json' },
  { path: 'apps/desktop/src-tauri/tauri.conf.json', format: 'json' },
  // Every crate inherits it with `version.workspace = true`.
  { path: 'Cargo.toml', format: 'cargo-workspace' },
];

const VERSION_PATTERN = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/;
// Chrome rejects manifest versions with a part above this.
const MAX_VERSION_PART = 65535;

/** Plain MAJOR.MINOR.PATCH: Chrome's manifest version takes no pre-release or build suffix. */
export function isValidVersion(version: string): boolean {
  const match = VERSION_PATTERN.exec(version);
  if (!match) return false;
  return match.slice(1).every((part) => Number(part) <= MAX_VERSION_PART);
}

export function readVersion({ text, format }: { text: string; format: VersionFormat }): string {
  return format === 'json' ? readJsonVersion(text) : readCargoWorkspaceVersion(text);
}

/** Returns the text with only the version changed, with LF line endings. */
export function writeVersion({
  text,
  format,
  version,
}: {
  text: string;
  format: VersionFormat;
  version: string;
}): string {
  const lfText = text.replace(/\r\n/g, '\n');
  return format === 'json'
    ? writeJsonVersion({ text: lfText, version })
    : writeCargoWorkspaceVersion({ text: lfText, version });
}

function readJsonVersion(text: string): string {
  const parsed: unknown = JSON.parse(text);
  const version = (parsed as { version?: unknown }).version;
  if (typeof version !== 'string') throw new Error('no top-level "version" string');
  return version;
}

// Top-level keys sit at two-space indent in every file here; nested "version" keys sit deeper.
const JSON_VERSION_LINE = /^( {2}"version"\s*:\s*")[^"]*(")/m;

function writeJsonVersion({ text, version }: { text: string; version: string }): string {
  if (!JSON_VERSION_LINE.test(text)) throw new Error('no top-level "version" line');
  const updated = text.replace(JSON_VERSION_LINE, `$1${version}$2`);
  const written = readJsonVersion(updated);
  if (written !== version) {
    throw new Error(`rewrote "version" but read back ${written}, not ${version}`);
  }
  return updated;
}

const CARGO_SECTION = '[workspace.package]';
const CARGO_VERSION_LINE = /^(version\s*=\s*")([^"]*)(".*)$/;

/** The index of the `version` line inside `[workspace.package]`. */
function findCargoVersionLine(lines: string[]): number {
  const sectionStart = lines.findIndex((line) => line.trim() === CARGO_SECTION);
  if (sectionStart === -1) throw new Error(`no ${CARGO_SECTION} section`);
  for (let index = sectionStart + 1; index < lines.length; index++) {
    const line = lines[index] ?? '';
    if (line.trim().startsWith('[')) break;
    if (CARGO_VERSION_LINE.test(line)) return index;
  }
  throw new Error(`no version in ${CARGO_SECTION}`);
}

function readCargoWorkspaceVersion(text: string): string {
  const lines = text.split(/\r?\n/);
  const line = lines[findCargoVersionLine(lines)] ?? '';
  return CARGO_VERSION_LINE.exec(line)?.[2] ?? '';
}

function writeCargoWorkspaceVersion({ text, version }: { text: string; version: string }): string {
  const lines = text.split('\n');
  const index = findCargoVersionLine(lines);
  lines[index] = (lines[index] ?? '').replace(CARGO_VERSION_LINE, `$1${version}$3`);
  return lines.join('\n');
}
