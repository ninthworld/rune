import js from '@eslint/js';
import globals from 'globals';
import reactHooks from 'eslint-plugin-react-hooks';
import reactRefresh from 'eslint-plugin-react-refresh';
import tseslint from 'typescript-eslint';
import prettier from 'eslint-config-prettier';

// Flat config (ESLint 9). Baseline is practical-strict, matching the Rust side:
// recommended type/JS rules + React hooks correctness, with Prettier owning
// formatting (eslint-config-prettier is last so it disables conflicting rules).
export default tseslint.config(
  { ignores: ['dist', 'node_modules'] },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  {
    files: ['**/*.{ts,tsx}'],
    languageOptions: {
      ecmaVersion: 2022,
      globals: globals.browser,
    },
    plugins: {
      'react-hooks': reactHooks,
      'react-refresh': reactRefresh,
    },
    rules: {
      ...reactHooks.configs.recommended.rules,
      'react-refresh/only-export-components': ['warn', { allowConstantExport: true }],
    },
  },
  {
    // The Playwright smoke canary (ADR 0011, issue #279) is Node-hosted test code:
    // it spawns the server, reads files, and drives a browser. Give it Node globals
    // (alongside the browser globals used inside `page.evaluate` callbacks) and drop
    // the component-only react-refresh rule that does not apply to test helpers.
    files: ['e2e/**/*.ts'],
    languageOptions: {
      globals: { ...globals.node, ...globals.browser },
    },
    rules: {
      'react-refresh/only-export-components': 'off',
    },
  },
  prettier,
);
