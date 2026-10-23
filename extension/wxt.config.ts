import { defineConfig } from 'wxt';

export default defineConfig({
  srcDir: 'src',
  manifestVersion: 3,
  manifest: {
    name: 'Hunch',
    description: 'Detects the key and BPM of the audio playing in a tab, locally.',
    permissions: ['tabCapture', 'offscreen', 'activeTab', 'storage', 'scripting', 'tabs'],
    host_permissions: ['<all_urls>'],
    content_security_policy: {
      extension_pages: "script-src 'self' 'wasm-unsafe-eval'; object-src 'self'",
    },
    action: {
      default_title: 'Toggle Hunch on this tab',
    },
    commands: {
      _execute_action: {
        suggested_key: { default: 'Alt+H' },
      },
    },
  },
  vite: () => ({
    // The analysis Worker imports the wasm-bindgen ES module, so it must be built as ESM.
    worker: { format: 'es' },
    build: {
      // Always emit .wasm as a file: a data: URL would bloat the Worker bundle.
      assetsInlineLimit: (filePath: string) => (filePath.endsWith('.wasm') ? false : undefined),
    },
  }),
});
