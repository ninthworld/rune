/**
 * Which server the pregame is pointed at, and what to call it (issue #546).
 *
 * The approved front-door and lobby baselines both put a server plaque on
 * screen, and the whole point of that plaque is the product's thesis: RUNE is a
 * dumb client, the default server is already chosen, and an ordinary player
 * configures nothing. So there is exactly one rule here — the configured default
 * reads as a *name*, and any other address is the player's own and is printed
 * verbatim rather than re-labelled as something it is not.
 *
 * Pure functions over the build-time configuration; no I/O and no state.
 */

/** Compile-time fallback when no `VITE_RUNE_SERVER_URL` is configured. */
export const DEFAULT_SERVER_URL = 'ws://localhost:9000';

/** The pre-filled server URL from the Vite env, else the fallback. */
export function initialServerUrl(): string {
  return import.meta.env.VITE_RUNE_SERVER_URL ?? DEFAULT_SERVER_URL;
}

/**
 * What the server plaque prints. `null`/empty means "not connected yet", which
 * on the front door is still the default server the one action will reach.
 */
export function serverLabel(url: string | null): string {
  const target = (url ?? '').trim();
  return target.length === 0 || target === initialServerUrl() ? 'Default Server' : target;
}
