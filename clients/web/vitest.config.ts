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
      // The terminal (game-over) counterpart fixture (issue #141): the client
      // asserts the same wire shape the Rust crate round-trips for a finished game.
      '@protocol-fixtures/gameview-over.json': fileURLToPath(
        new URL('../../crates/rune-protocol/fixtures/gameview-over.json', import.meta.url),
      ),
      // The prompt-shapes fixture (issue #156): a pre-game mulligan frame carrying
      // the `option` + `select_from_zone` prompts the server projects, round-tripped
      // by the Rust crate and asserted here so the wire shapes cannot drift.
      '@protocol-fixtures/gameview-prompts.json': fileURLToPath(
        new URL('../../crates/rune-protocol/fixtures/gameview-prompts.json', import.meta.url),
      ),
      // The Commander presentation fixture (issue #553): a mid-game Commander frame
      // whose command zones are all empty, so the format signal, the per-seat
      // commander identity and the per-permanent marker are the only things that can
      // carry the presentation. Round-tripped by the Rust crate and asserted here.
      '@protocol-fixtures/gameview-commander.json': fileURLToPath(
        new URL('../../crates/rune-protocol/fixtures/gameview-commander.json', import.meta.url),
      ),
      // The action-contract fixture (issue #554): contextual labels, a submission
      // acknowledgement, a numeric prompt, and server-authoritative destinations
      // (including actions that name none). Round-tripped by the Rust crate.
      '@protocol-fixtures/gameview-actions.json': fileURLToPath(
        new URL('../../crates/rune-protocol/fixtures/gameview-actions.json', import.meta.url),
      ),
    },
  },
  test: {
    environment: 'jsdom',
    setupFiles: ['./src/test/setup.ts'],
    // `scripts/` holds the build-output gates (issue #510): plain ESM JavaScript,
    // because CI runs them with bare `node` against `dist/` with no bundler in
    // the loop. Their pure halves are unit-tested here alongside the client.
    include: ['src/**/*.test.{ts,tsx}', 'scripts/**/*.test.js'],
  },
});
