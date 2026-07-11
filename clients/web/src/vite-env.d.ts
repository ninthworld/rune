/**
 * Ambient typing for the Vite build-time env vars this client reads. Only
 * `VITE_*` names are exposed to client code (Vite convention); this declares the
 * one the connection screen consumes for its default server URL.
 */
interface ImportMetaEnv {
  /** Default RUNE server WebSocket URL; falls back to `ws://localhost:9000`. */
  readonly VITE_RUNE_SERVER_URL?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
