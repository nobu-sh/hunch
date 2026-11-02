import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';

// Dev page that renders the overlay from fixture views. Run `pnpm preview` from packages/ui.
export default defineConfig({
  root: 'preview',
  plugins: [react()],
});
