import { fileURLToPath } from 'node:url';
import { defineConfig } from 'vitest/config';

// jsdom covers both suites: the card-factory smoke tests build real Pixi display
// objects (no GPU/GL — src/test/setup.ts stubs the 2D canvas context), and the
// store/wire tests are environment-agnostic (fake sockets; localStorage is stubbed
// per-test). No GPU/GL is used in CI.
//
// The `@protocol-fixtures/gameview.json` alias resolves to the single canonical
// contract fixture owned by the `rune-protocol` crate, so the TS mirror test reads
// the exact same bytes the Rust round-trip test does (issue #56). The matching
// path lives in tsconfig.json so `tsc` resolves the import too.
export default defineConfig({
  resolve: {
    alias: {
      '@protocol-fixtures/gameview.json': fileURLToPath(
        new URL('../../crates/rune-protocol/fixtures/gameview.json', import.meta.url),
      ),
    },
  },
  test: {
    environment: 'jsdom',
    setupFiles: ['./src/test/setup.ts'],
    include: ['src/**/*.test.{ts,tsx}'],
  },
});
