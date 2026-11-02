// `pnpm desktop:dev`: `tauri dev` with WebView2's remote debugging port open, so the dev
// browser can attach to the app's page.
import { run } from '@tauri-apps/cli';
import { WEBVIEW2_DEBUG_PORT } from './debug-port.ts';

process.env.WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = `--remote-debugging-port=${String(WEBVIEW2_DEBUG_PORT)}`;
await run(['dev', ...process.argv.slice(2)], 'tauri');
