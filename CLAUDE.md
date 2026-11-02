# Hunch

Detects the key and BPM of audio as it plays. A Rust analysis core runs natively (eval CLI) and as WASM inside a Chromium MV3 extension.

## Layout

- `crates/hunch-core`: DSP and models (key, tempo, listening windows). No IO.
- `crates/hunch-wasm`: wasm-bindgen wrapper the extension loads.
- `crates/hunch-eval`: native CLI that scores the core against datasets in `data/` (gitignored). Run `cargo run -p hunch-eval --release -- --help`.
- `extension/`: WXT + TypeScript. Its pnpm commands run from `extension/`.

## Checks (same as CI)

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
# in extension/
pnpm lint && pnpm format:check && pnpm typecheck && pnpm test
pnpm build:wasm && pnpm build     # builds extension/.output/chrome-mv3
```

## Running the extension in a browser

`pnpm browser` loads the built extension into Chrome for Testing and drives it with Puppeteer. It never touches your own Chrome profile.

```sh
pnpm browser:install                  # once: downloads Chrome for Testing into extension/.chrome
pnpm build:wasm && pnpm build         # the harness runs the last build
pnpm browser:check                    # headless smoke test: built-in 120 BPM A minor beat, exit 1 on a miss
pnpm browser --url <url> --seconds 20 # any page; prints overlay readouts as they change
pnpm browser --screenshot shot.png    # screenshot of the page with the overlay
pnpm browser --headed --keep-open     # visible window, left open to poke at
```

- It clicks the toolbar icon with Puppeteer's `triggerAction`, which counts as the user action tab capture needs.
- It reads the overlay through DevTools, because the overlay's shadow root is closed.
- The test page plays through an `<audio>` element.
  - Tab capture doesn't hear Web Audio contexts that were already running when capture started.
- Audio plays through the speakers while it runs.
  - Puppeteer's default `--mute-audio` also silences tab capture, so the harness turns it off.
- Branded Chrome ignores unpacked extensions loaded from the command line, so the harness uses Chrome for Testing.
