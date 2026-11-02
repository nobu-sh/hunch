# Hunch

Detects the key and BPM of audio as it plays. A Rust analysis core runs natively (eval CLI) and as WASM inside a Chromium MV3 extension.

## Layout

- `crates/hunch-core`: DSP and models (key, tempo, listening windows). No IO.
- `crates/hunch-wasm`: wasm-bindgen wrapper the extension loads.
- `crates/hunch-eval`: native CLI that scores the core against datasets in `data/` (gitignored). Run `cargo run -p hunch-eval --release -- --help`.
- `extension/`: WXT + TypeScript. Its build and browser commands run from `extension/`.
- `packages/ui` (`@hunch/ui`): shared UI logic and components.
- `packages/harness` (`@hunch/harness`): Chrome for Testing harness. Generic parts (install, launch or attach over DevTools, closed shadow roots, console errors, screenshots, the test-beat page) in `src/`, the extension driver and CLI in `src/extension/`.
- The repo root is a pnpm workspace (`extension`, `packages/*`) with one lockfile. Run `pnpm install` from the root.

## Checks (same as CI)

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm install --frozen-lockfile
pnpm lint && pnpm format:check && pnpm typecheck && pnpm test   # every workspace package
# in extension/
pnpm build:wasm && pnpm build     # builds extension/.output/chrome-mv3
```

## Running the extension in a browser

`pnpm browser` launches Chrome for Testing with the built extension, opens the test-beat page (or `--url`), turns Hunch on for it, and prints the extension's session state for the tab (from `chrome.storage.session`) as it changes. It's headed unless `--headless` is passed, stays open until you close it (or `--seconds <n>`), and writes its DevTools endpoint to `.chrome/devtools-endpoint.txt` at the repo root. It never touches your own Chrome profile. The commands are thin calls into `@hunch/harness`'s `hunch-harness` CLI and work from `extension/` or the repo root.

```sh
pnpm browser:install          # once: downloads Chrome for Testing into .chrome at the repo root
pnpm build:wasm && pnpm build # the harness runs the last build
pnpm browser                  # headed, test beat, Hunch on, stays open
pnpm browser:check            # headless plumbing check: the test beat reaches the analyzer, exit 1 if not
pnpm browser --url <url>      # any page instead of the test beat
pnpm browser --connect 9222   # attach to a running browser by port, http URL or ws endpoint
```

To look at or poke the UI, script standard Puppeteer against the open browser. Save the script in `packages/harness`, so it resolves `puppeteer-core`, and run it with `node` from there:

```js
import puppeteer from 'puppeteer-core';
import { readFileSync } from 'node:fs';
const endpoint = readFileSync('../../.chrome/devtools-endpoint.txt', 'utf8').trim();
const browser = await puppeteer.connect({ browserWSEndpoint: endpoint, defaultViewport: null });
const page = (await browser.pages()).find((tab) => tab.url().startsWith('http'));
await page.screenshot({ path: 'shot.png' }); // or: (await page.$('hunch-overlay')).screenshot(...)
await page.mouse.click(1180, 38); // viewport x, y
console.log(await page.evaluate(() => document.title));
await browser.disconnect(); // leaves the browser open
```

- The harness knows nothing about the overlay's markup, so UI changes never need harness changes. Check UI headed: open it, take screenshots, and look at them. Model accuracy is tested against the core with `hunch-eval`, not through the browser.
- `browser:check` passes when the tab's session has a window with estimates, a tempo near 120 BPM (or half or double), and no session or console errors. The key isn't judged.
- It clicks the toolbar icon with Puppeteer's `triggerAction`, which counts as the user action tab capture needs.
- The test page (120 BPM, A minor) plays through an `<audio>` element.
  - Tab capture doesn't hear Web Audio contexts that were already running when capture started.
- Audio plays through the speakers while it runs.
  - Puppeteer's default `--mute-audio` also silences tab capture, so the harness turns it off.
- `--connect` needs the browser started with `--enable-unsafe-extension-debugging` to install the extension. It opens its own tab and only detaches at the end. `openBrowser({ connect })` takes a WebView2 debug port the same way, for the desktop app later.
- Branded Chrome ignores unpacked extensions loaded from the command line, so the harness uses Chrome for Testing.
