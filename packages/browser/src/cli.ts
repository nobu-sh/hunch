#!/usr/bin/env node
// `hunch-browser`: a dev browser for agents. Node runs this file directly by stripping its types.
import { installChrome } from './chrome.ts';
import { loadMixin } from './mixin.ts';
import { DEFAULT_OPTIONS, mergeOptions, parseCommandLine, resolvePaths } from './options.ts';
import { runBrowser } from './run.ts';

const HELP = `Usage: hunch-browser [options]
       hunch-browser install

Opens Chrome for Testing (headed unless --headless), loads any unpacked extensions, opens a URL
and runs a mixin. It stays open until the browser closes or --seconds run out, and writes its
DevTools endpoint to --endpoint-file while it runs, so Puppeteer scripts can attach.

Options:
  --url <url>              Page to open in the first tab.
  --headless               Run without a window.
  --connect <target>       Attach to a running browser instead: a debugging port, an http(s)
                           DevTools URL or a ws(s) endpoint. Installing extensions into it
                           needs --enable-unsafe-extension-debugging.
  --extension <dir>        Unpacked extension to load. Repeatable.
  --seconds <n>            Close after n seconds.
  --window-size <w>x<h>    Window size, e.g. 1280x800. The page size when headless.
  --devtools               Open DevTools for the page.
  --profile <dir>          Keep the browser profile in this folder. Default: a fresh one.
  --mute                   Mute the browser. Off by default: muting also silences tab capture.
  --console <mode>         Print console errors (default), all messages, or none.
  --endpoint-file <path>   Default: ${DEFAULT_OPTIONS.endpointFile}
  --chrome-arg <flag>      Extra Chrome flag, e.g. --chrome-arg=--lang=fr. Repeatable.
  --mixin <file>           A .ts or .js module whose default export is defineMixin({...}).
                           Its options are defaults; flags given here override them.
  --install, install       Download Chrome for Testing, then exit.
  --help                   Show this help.`;

async function main(args: string[]): Promise<number> {
  const commandLine = parseCommandLine(args);
  if (commandLine.isHelp) {
    console.log(HELP);
    return 0;
  }
  if (commandLine.isInstall) {
    await installChrome();
    return 0;
  }

  const mixin = commandLine.mixin ? await loadMixin(commandLine.mixin) : null;
  const options = mergeOptions({
    mixin: mixin?.options ?? {},
    commandLine: resolvePaths({ options: commandLine.options, baseDir: process.cwd() }),
  });
  return runBrowser({
    options,
    mixin,
    log: (line) => {
      console.log(line);
    },
  });
}

try {
  process.exitCode = await main(process.argv.slice(2));
} catch (error) {
  console.error(error instanceof Error ? error.message : error);
  process.exitCode = 1;
}
