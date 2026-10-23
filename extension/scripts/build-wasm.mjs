// Builds crates/hunch-wasm and generates the JS bindings into src/wasm/pkg.
// Plain Node so it runs the same from cmd.exe and POSIX shells.
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const repoRoot = fileURLToPath(new URL('../../', import.meta.url));
const wasmTarget = 'wasm32-unknown-unknown';

function run(command, args) {
  console.log(`> ${command} ${args.join(' ')}`);
  const result = spawnSync(command, args, { cwd: repoRoot, stdio: 'inherit' });
  if (result.error) {
    console.error(`Failed to start ${command}: ${result.error.message}`);
    if (command === 'wasm-bindgen') {
      console.error(
        'Install the CLI matching the wasm-bindgen version in Cargo.lock: ' +
          'cargo install wasm-bindgen-cli --version <version>',
      );
    }
    process.exit(1);
  }
  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
}

run('cargo', ['build', '-p', 'hunch-wasm', '--target', wasmTarget, '--release']);
run('wasm-bindgen', [
  '--target',
  'web',
  '--out-dir',
  'extension/src/wasm/pkg',
  `target/${wasmTarget}/release/hunch_wasm.wasm`,
]);
