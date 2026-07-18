/**
 * Card-art source preference (ADR 0024): which art pipeline the player opted
 * into on THIS device. This is a device-local presentation setting — never game
 * state — so `localStorage` is the right home (the client AGENTS.md rule forbids
 * storing *game state*, not preferences). Storage access is guarded: where it is
 * unavailable (privacy modes, SSR, tests without a stub) the setting degrades to
 * the default and saving is a no-op.
 */

/**
 * The three art pipelines (ADR 0024):
 * - `procedural` — the default vector frame with the monogram art-box placeholder;
 *   no images, works offline, identical to the pre-art client.
 * - `bundled` — project-owned art shipped with the client under
 *   `/card-art/<functional_id>.jpg` (the RUNE-generated set, when present).
 * - `scryfall` — real card images the PLAYER opts into: fetched by their browser
 *   directly from Scryfall, cached only on their device, never shipped or
 *   redistributed by the project.
 */
export type ArtSource = 'procedural' | 'bundled' | 'scryfall';

/** `localStorage` key for the device-local art source preference. */
const ART_SOURCE_KEY = 'rune.cardArtSource';

/** Whether a stored string names a known art source. */
function isArtSource(value: string | null): value is ArtSource {
  return value === 'procedural' || value === 'bundled' || value === 'scryfall';
}

/**
 * The persisted art source, defaulting to `procedural` when nothing is stored
 * or storage is unavailable. Selecting `scryfall` is only ever done by the
 * player through the settings surface, so a stored `scryfall` IS the opt-in
 * consent record (ADR 0024).
 */
export function loadArtSource(): ArtSource {
  try {
    const stored = localStorage.getItem(ART_SOURCE_KEY);
    return isArtSource(stored) ? stored : 'procedural';
  } catch {
    return 'procedural';
  }
}

/** Persist the art source; a no-op where storage is unavailable. */
export function saveArtSource(source: ArtSource): void {
  try {
    localStorage.setItem(ART_SOURCE_KEY, source);
  } catch {
    // Storage unavailable — the choice simply doesn't survive a reload.
  }
}
