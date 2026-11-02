import { resolve } from 'node:path';

/** Resolves `path` against the folder the command was typed in, not the package pnpm runs in. */
export function fromInvocationDir(path) {
  // pnpm runs scripts from the package folder; INIT_CWD is where the command was typed.
  return resolve(process.env.INIT_CWD ?? process.cwd(), path);
}
