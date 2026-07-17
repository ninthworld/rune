/**
 * Join CSS-module class names, dropping falsy entries. The chrome styling layer
 * (ADR 0019) composes an element's look from a base class plus zero or more
 * modifier classes selected at render time (`cx(styles.tile, isLocal && styles.localTile)`),
 * replacing the old inline-style object spreads. Purely a string joiner — it holds
 * no game logic and no styling values of its own.
 */
export function cx(...names: Array<string | false | null | undefined>): string {
  return names.filter(Boolean).join(' ');
}
