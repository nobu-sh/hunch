import js from '@eslint/js';
import globals from 'globals';

// Node, plus browser globals for the callbacks puppeteer runs inside pages.
export default [
  js.configs.recommended,
  { languageOptions: { globals: { ...globals.node, ...globals.browser } } },
];
