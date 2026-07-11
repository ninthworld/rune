/**
 * Test-only `window` hook for the browser e2e suite (ADR 0011).
 *
 * The Pixi canvas is a pure visual surface whose output is fully determined by
 * the {@link TableScene} the client already computes (ADR 0003). Rather than
 * decode pixels, an e2e test reads that derived scene here and asserts on
 * structured facts ("Grizzly Bears is in the local band, tapped, with two +1/+1
 * counters"). The hook publishes **derived render data only** — it is strictly
 * read-only, is not a control channel, and adds no logic to any production code
 * path, so it does not touch the "zero game logic in the client" or
 * "rebuild from one GameView" invariants.
 *
 * It is gated behind the build-time `VITE_RUNE_TEST_HOOKS` flag, which is only
 * set for the e2e preview build. In a normal production build the flag is
 * statically `undefined`, so {@link publishScene} short-circuits and never
 * touches `window` — the hook cannot ship live to players.
 */
import type { TableScene } from './table/scene';

/** Whether the test hook is compiled in (build-time flag; `false` in production). */
export const TEST_HOOKS_ENABLED = Boolean(import.meta.env.VITE_RUNE_TEST_HOOKS);

/** The namespaced, read-only surface the e2e suite reads via `page.evaluate`. */
export interface RuneTestHook {
  /** The latest {@link TableScene} the canvas is drawing, or `null` pre-first-frame. */
  scene: TableScene | null;
}

declare global {
  interface Window {
    /** Present only in e2e/preview builds; see {@link RuneTestHook}. */
    __RUNE_TEST__?: RuneTestHook;
  }
}

/**
 * Publish the current scene on `window.__RUNE_TEST__` when the test hook is
 * enabled. A no-op in production builds and where there is no `window`.
 */
export function publishScene(scene: TableScene | null): void {
  if (!TEST_HOOKS_ENABLED || typeof window === 'undefined') return;
  const hook: RuneTestHook = window.__RUNE_TEST__ ?? { scene: null };
  hook.scene = scene;
  window.__RUNE_TEST__ = hook;
}
