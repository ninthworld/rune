/**
 * Ambient typing for the read-only e2e scene hook (ADR 0011), mirroring
 * `src/testHooks.ts`. Declared here for the e2e program so tests can read
 * `window.__RUNE_TEST__.scene` with the real {@link TableScene} type without
 * pulling the production module (and its Vite `import.meta.env`) into `tsc`.
 */
import type { TableScene } from '../src/table/scene';

declare global {
  interface Window {
    /** Present only in e2e/preview builds; the scene the canvas is drawing. */
    __RUNE_TEST__?: { scene: TableScene | null };
  }
}
