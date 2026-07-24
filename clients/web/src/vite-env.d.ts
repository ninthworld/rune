/**
 * Ambient typing for the Vite build-time env vars this client reads. Only
 * `VITE_*` names are exposed to client code (Vite convention); this declares the
 * one the connection screen consumes for its default server URL.
 */
interface ImportMetaEnv {
  /** Vite's built-in development-mode flag. */
  readonly DEV: boolean;
  /** Default RUNE server WebSocket URL; falls back to `ws://localhost:9000`. */
  readonly VITE_RUNE_SERVER_URL?: string;
  /**
   * Set only for the e2e preview build (ADR 0011) to compile in the read-only
   * `window.__RUNE_TEST__` scene hook. Unset (and thus statically `undefined`)
   * in production builds, so the hook is never live for players.
   */
  readonly VITE_RUNE_TEST_HOOKS?: string;
  /**
   * Enables the isolated `/fixtures/2.5d` integration route in a production
   * preview build. Development builds enable it automatically; normal
   * production builds leave it inaccessible.
   */
  readonly VITE_RUNE_FIXTURE_HARNESS?: string;
  /**
   * Opts real in-game routes into the ADR 0030 2.5D composition while the
   * legacy Pixi table remains the safe default through the Phase 2 exit.
   */
  readonly VITE_RUNE_2_5D_MATCH?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}

/**
 * CSS-module imports (ADR 0019 chrome styling layer): the default export maps each
 * authored class name to its build-time-scoped class string. Global `.css` imports
 * (tokens/base) are side-effecting and carry no exports.
 */
declare module '*.module.css' {
  const classes: { readonly [key: string]: string };
  export default classes;
}

declare module '*.css';
