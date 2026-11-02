import js from '@eslint/js';
import globals from 'globals';

// Node, plus browser and extension globals for the callbacks puppeteer runs inside pages and
// extension workers.
export default [
  js.configs.recommended,
  {
    languageOptions: { globals: { ...globals.node, ...globals.browser, ...globals.webextensions } },
  },
];
