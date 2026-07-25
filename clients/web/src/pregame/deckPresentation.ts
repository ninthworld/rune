/**
 * Display-only reads of the bundled starter decklists, for the ready room's
 * deck choice (issue #506; trimmed to its one surviving read by #546, which
 * replaced the deck-tile grid with a dropdown).
 *
 * **No card logic.** The bundled decklists are static names/ids
 * (`decklists.ts`); this only looks at that static data to name which of a
 * deck's own rows its designated commander is. Cost, legality, and effect are
 * never touched here — the server validates a submitted deck authoritatively
 * behind the unchanged `submit_deck` gate.
 */
import type { Decklist } from '../decklists';

/**
 * The display name of a deck's designated commander (issue #372): resolved from
 * the deck's own rows by matching identity, presentation only. `undefined` when
 * the deck designates none, or the identity is somehow not among its rows.
 */
export function commanderName(deck: Decklist): string | undefined {
  if (deck.commander === undefined) return undefined;
  return deck.entries.find((entry) => entry.identity === deck.commander)?.name;
}
