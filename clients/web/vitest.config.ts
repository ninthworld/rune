import { defineConfig } from 'vitest/config';

// Unit tests for the networking store and wire parsing. The store is driven with
// injected fake sockets, so a plain Node environment (no jsdom) is sufficient.
export default defineConfig({
  test: {
    environment: 'node',
    include: ['src/**/*.test.ts'],
  },
});
