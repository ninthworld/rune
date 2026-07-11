/**
 * Ambient typing for the Vite build-time env vars this client reads. Only
 * `VITE_*` names are exposed to client code (Vite convention); this declares the
 * one the connection screen consumes for its default server URL.
 */
interface ImportMetaEnv {
  /** Default RUNE server WebSocket URL; falls back to `ws://localhost:9000`. */
  readonly VITE_RUNE_SERVER_URL?: string;
  /**
   * Set only for the e2e preview build (ADR 0011) to compile in the read-only
   * `window.__RUNE_TEST__` scene hook. Unset (and thus statically `undefined`)
   * in production builds, so the hook is never live for players.
   */
  readonly VITE_RUNE_TEST_HOOKS?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
