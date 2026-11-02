# Hunch

Detects the key and BPM of audio as it plays. A Rust analysis core runs natively (eval CLI, desktop app) and as WASM inside a Chromium MV3 extension.

## Layout

- `crates/hunch-core`: DSP and models (key, tempo, listening windows). No IO. Its `serde` feature adds `contract`: the `Estimate` / `SessionEvent` JSON every frontend consumes, mirrored in `packages/ui/src/estimate.ts`.
- `crates/hunch-wasm`: wasm-bindgen wrapper the extension loads.
- `crates/hunch-capture`: audio capture for the desktop app. It has a platform-independent `AudioBackend` trait and source model, converts audio to mono 22,050 Hz chunks, and has a WASAPI backend in `platform/windows` (other platforms get a stub). Its CLI lists sources and prints live session events: `cargo run -p hunch-capture --release -- list`, then `listen <id> [--seconds n]`.
- `crates/hunch-eval`: native CLI that scores the core against datasets in `data/` (gitignored). Run `cargo run -p hunch-eval --release -- --help`.
- `extension/`: WXT + TypeScript. Its build commands run from `extension/`. `extension/dev/` holds the dev-only Hunch browser mixins and the test-beat page.
- `apps/desktop`: the Tauri 2 desktop app (Windows first). `src-tauri/` is the Rust backend, a workspace crate that runs a `hunch-core` `Session` from an `AudioFeed` and emits each event as a `session-event` Tauri event; for now the feed is a synthesized test beat (120 BPM, A minor). `src/` is the React UI built from `@hunch/ui`; `dev/` holds its browser mixin and dev script.
- `packages/ui` (`@hunch/ui`): shared UI logic and components.
- `packages/browser` (`@hunch/browser`): a generic dev browser for agents, with no Hunch knowledge. It handles Chrome for Testing, launching or attaching, unpacked extensions, the DevTools endpoint file, console output and mixins. TypeScript that Node runs directly by stripping types: no build step.
- The repo root is a pnpm workspace (`apps/*`, `extension`, `packages/*`) with one lockfile. Run `pnpm install` from the root.

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

On Linux, building `apps/desktop/src-tauri` needs Tauri's system packages; CI installs them with `.github/actions/tauri-linux-deps`.

## Desktop app

```sh
pnpm desktop:dev      # opens the app: Vite dev server + tauri dev, WebView2 debug port 9230
pnpm desktop:build    # release build: target/release/hunch-desktop.exe (no installer yet)
pnpm browser:desktop  # with desktop:dev running: attach to the app's page and print its key and BPM
```

- The window is the overlay: transparent, undecorated, and resized to the panel or pill. Drag the header or pill to move it; close quits.
- The UI subscribes to `session-event`, then calls the `listen` command, which starts a fresh session so no event is missed.
- Tauri is pinned to 2.11 (crate and npm) because 2.12 needs rustc 1.90.

## Running the extension in a browser

`pnpm browser` (the `hunch-browser` CLI) opens Chrome for Testing headed, loads unpacked extensions, opens a URL and runs an optional mixin. It stays open until the browser closes or `--seconds` run out. Run the commands from the repo root.

```sh
pnpm browser install                 # once: downloads Chrome for Testing into .chrome at the repo root
pnpm build:wasm && pnpm build        # in extension/: the mixins load the last build
pnpm browser:extension               # headed: test beat, Hunch on, session state printed as it changes
pnpm browser:check                   # headless plumbing check: exit 1 unless the beat reaches the analyzer
pnpm browser --url https://example.com --extension extension/.output/chrome-mv3
```

Flags: `--url`, `--headless`, `--connect <port|http(s) URL|ws(s) endpoint>`, `--reuse-page` (with `--connect`, drive the first open page instead of a new tab), `--extension <dir>` (repeatable), `--seconds <n>`, `--window-size <w>x<h>`, `--devtools`, `--profile <dir>` (default: a fresh profile), `--mute`, `--console errors|all|none`, `--endpoint-file <path>`, `--chrome-arg <flag>` (repeatable), `--mixin <file>`. Flags override a mixin's defaults; see `pnpm browser --help`.

While it runs, a launched browser's DevTools endpoint is in `.chrome/devtools-endpoint.txt`. To look at or poke the UI, script standard Puppeteer against it. Save the script in `packages/browser` so it resolves `puppeteer-core`, and run it with `node`:

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

A mixin is a `.ts` or `.js` module that sets default options and drives the open browser. Paths in its options resolve from the mixin's folder. Returning a number exits with it; returning nothing stays open:

```ts
import { defineMixin } from '@hunch/browser';

export default defineMixin({
  options: { url: 'https://example.com', extensions: ['../.output/chrome-mv3'] },
  async run({ page, extensions, log }) {
    await extensions[0]?.handle.triggerAction(page);
    log(await page.title());
  },
});
```

The context also has `browser`, `options`, `endpoint`, the console `errors` collected so far, a `signal` that aborts when the browser closes or time runs out, and `seconds`.

- `apps/desktop/dev/browser.ts` (`pnpm browser:desktop`) connects to port 9230 with `reusePage`, so it drives the app's own page. A Puppeteer script can attach the same way: `puppeteer.connect({ browserURL: 'http://127.0.0.1:9230', defaultViewport: null })`. Nothing writes an endpoint file for an attached browser.
- The Hunch mixins live in `extension/dev/`:
  - `browser.ts` (`pnpm browser:extension`): the build on the test beat. It turns Hunch on and prints the tab's session state from `chrome.storage.session` until the browser closes.
  - `browser-check.ts` (`pnpm browser:check`): headless for 12 s. It passes on a window with estimates, a tempo near 120 BPM (or half or double), and no session or console errors. The key isn't judged.
- Check UI headed: open it, take screenshots, and look at them. Model accuracy is tested against the core with `hunch-eval`, not through the browser.
- Branded Chrome ignores unpacked extensions loaded from the command line, so this uses Chrome for Testing. Extensions are installed after launch with `browser.installExtension`, because passing them to `launch` doesn't finish loading them. A toolbar click before the service worker runs is lost, so loading waits for it.
- Audio isn't muted unless you pass `--mute`: muting Chrome also silences tab capture.
- The test page (120 BPM, A minor) plays through an `<audio>` element. Tab capture doesn't hear Web Audio contexts that were already running when capture started.
- `--connect` needs the browser started with `--enable-unsafe-extension-debugging` to install extensions. It opens its own tab unless `--reuse-page` is set, and only detaches at the end. A WebView2 debug port works the same way, but WebView2 can't install extensions.
