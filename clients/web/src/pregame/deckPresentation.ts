/**
 * Display-only reads of the bundled starter decklists, for the room's deck
 * column (issue #506; carried unchanged from the shipped lobby).
 *
 * **No card logic.** The bundled decklists are static names/ids
 * (`decklists.ts`); these helpers only look at that static data to decide which
 * land glyphs a tile shows and which of a deck's own rows its designated
 * commander is. Cost, legality, and effect are never touched here — the server
 * validates a submitted deck authoritatively behind the unchanged `submit_deck`
 * gate.
 */
import type { GlyphName } from '../chrome/glyphs';
import type { Decklist } from '../decklists';
import { PALETTE } from '../tokens';

/** One basic land's glyph and its card-token frame hue. */
export interface LandGlyph {
  readonly name: string;
  readonly glyph: GlyphName;
  readonly hue: string;
}

/**
 * The basic-land glyphs a deck tile can show, in WUBRG order, tinted by the
 * card-token frame hue — the same "what colors" read the table's land chips
 * give.
 */
export const BASIC_LAND_GLYPHS: readonly LandGlyph[] = [
  { name: 'Plains', glyph: 'land-plains', hue: PALETTE.W },
  { name: 'Island', glyph: 'land-island', hue: PALETTE.U },
  { name: 'Swamp', glyph: 'land-swamp', hue: PALETTE.B },
  { name: 'Mountain', glyph: 'land-mountain', hue: PALETTE.R },
  { name: 'Forest', glyph: 'land-forest', hue: PALETTE.G },
];

/** The land glyphs (with hues) for the basics a decklist actually runs. */
export function deckLandGlyphs(deck: Decklist): readonly LandGlyph[] {
  return BASIC_LAND_GLYPHS.filter((land) => deck.entries.some((e) => e.name === land.name));
}

/**
 * The display name of a deck's designated commander (issue #372): resolved from
 * the deck's own rows by matching identity, presentation only. `undefined` when
 * the deck designates none, or the identity is somehow not among its rows.
 */
export function commanderName(deck: Decklist): string | undefined {
  if (deck.commander === undefined) return undefined;
  return deck.entries.find((entry) => entry.identity === deck.commander)?.name;
}
