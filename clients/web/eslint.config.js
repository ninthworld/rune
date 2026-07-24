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
  prettier,
);
