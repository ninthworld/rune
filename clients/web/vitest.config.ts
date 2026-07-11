import { defineConfig } from 'vitest/config';

// jsdom covers both suites: the card-factory smoke tests build real Pixi display
// objects (no GPU/GL — src/test/setup.ts stubs the 2D canvas context), and the
// store/wire tests are environment-agnostic (fake sockets; localStorage is stubbed
// per-test). No GPU/GL is used in CI.
export default defineConfig({
  test: {
    environment: 'jsdom',
    setupFiles: ['./src/test/setup.ts'],
    include: ['src/**/*.test.{ts,tsx}'],
  },
});
