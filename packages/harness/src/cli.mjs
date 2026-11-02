#!/usr/bin/env node
// Entry point for the harness commands: `install` fetches Chrome for Testing, `extension` drives
// the built extension.
import { installChrome } from './chrome.mjs';
import { runExtension } from './extension/cli.mjs';

const USAGE = `Usage: hunch-harness <command> [options]

Commands:
  install     Download Chrome for Testing into the repo's .chrome folder.
  extension   Run the built extension in Chrome for Testing. See: extension --help`;

async function main([command, ...args]) {
  switch (command) {
    case 'install':
      await installChrome();
      return 0;
    case 'extension':
      return runExtension(args);
    default:
      console.error(USAGE);
      return 1;
  }
}

try {
  process.exitCode = await main(process.argv.slice(2));
} catch (error) {
  console.error(error instanceof Error ? error.message : error);
  process.exitCode = 1;
}
