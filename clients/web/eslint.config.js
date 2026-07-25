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
  // Build output and installed packages are never linted. `dist-e2e` is the
  // hooks-enabled preview build the browser suite serves (ADR 0011); it lands
  // beside `dist` so `npm run budget` keeps measuring the shipped artifact, and
  // it has to be ignored here for the same reason `dist` is — otherwise a
  // developer who has run `make e2e` finds `npm run lint` reporting thousands of
  // errors in a minified bundle.
  { ignores: ['dist', 'dist-e2e', 'node_modules'] },
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
  // Build-output gates run under Node against `dist/` (issue #510).
  {
    files: ['scripts/**/*.js'],
    languageOptions: {
      ecmaVersion: 2022,
      globals: globals.node,
    },
  },
  // The device probe is pasted into a browser console on the device under test,
  // so it is browser code that happens to live beside the Node scripts.
  {
    files: ['scripts/deviceBudgetProbe.js', 'scripts/deviceBudgetProbe.test.js'],
    languageOptions: {
      globals: globals.browser,
    },
  },
  // The browser smoke suite (`e2e/`, ADR 0011 / issue #279) is Playwright code:
  // it runs in Node but its `page.evaluate` callbacks are browser code, so it
  // needs both global sets. Two React rules do not apply to it — Playwright's
  // fixture signature is `async ({}, use)`, which is neither an empty pattern
  // bug nor a React hook.
  {
    files: ['e2e/**/*.ts'],
    languageOptions: {
      globals: { ...globals.browser, ...globals.node },
    },
    rules: {
      'no-empty-pattern': 'off',
      'react-hooks/rules-of-hooks': 'off',
    },
  },
  prettier,
);
