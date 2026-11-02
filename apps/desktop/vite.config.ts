import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';

/** Must match `build.devUrl` in src-tauri/tauri.conf.json. */
const DEV_PORT = 1420;

export default defineConfig({
  plugins: [react()],
  // Tauri prints its own output; keep Vite's errors visible above it.
  clearScreen: false,
  server: { port: DEV_PORT, strictPort: true },
});
