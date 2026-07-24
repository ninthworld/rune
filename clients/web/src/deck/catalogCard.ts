/**
 * Browse-time card adapters for the deck builder (issue #508): map a wire-carried
 * {@link CatalogCard} onto the shared presentation contracts so the builder draws
 * every card through the one DOM card renderer ({@link CardFace}) and the one
 * inspect surface ({@link CardInspect}) — no bespoke card markup of its own.
 *
 * Pure field mapping, **no game logic** (AGENTS.md, ADR 0012): every characteristic
 * (cost, type line, rules text, P/T, keywords) is server-computed and rendered
 * verbatim; the only derivations are the same display glue the table already uses —
 * the frame {@link deriveColorIdentity} and the basic-land chip glyph — neither of
 * which decides legality, cost, or effect.
 */
import type { CardDisplayData } from '../card/cardFactory';
import type { CardView, CatalogCard } from '../protocol';
import { deriveColorIdentity } from '../table/colorIdentity';
import { basicLandGlyph, hasActivatedAbilityText } from '../table/scene/card-helpers';
import { artKeyFor } from '../card/art/artStore';

/**
 * Adapt a browse-time {@link CatalogCard} into the in-game {@link CardView} shape the
 * shared {@link CardInspect} renders. The catalog entry names a card by identity, so
 * the per-game entity `id` is stood in by the `functional_id` (nothing in inspect
 * treats it as an entity handle). No characteristic is derived.
 */
export function catalogCardToView(card: CatalogCard): CardView {
  return {
    id: card.functional_id,
    name: card.name,
    type_line: card.type_line,
    mana_cost: card.mana_cost,
    rules_text: card.rules_text,
    functional_id: card.functional_id,
    power: card.power,
    toughness: card.toughness,
    keywords: card.keywords,
  };
}

/**
 * Map a {@link CatalogCard} onto the shared {@link CardDisplayData} the DOM card face
 * consumes — the same contract the live table builds from a {@link CardView}. The
 * builder is a pre-game surface with no permanent state, so every combat/permanent
 * flag is absent; the frame color and basic-land glyph are the table's own display
 * glue, and player-side art (ADR 0024) is only looked up, never fetched.
 */
export function catalogCardToDisplayData(card: CatalogCard): CardDisplayData {
  return {
    name: card.name,
    typeLine: card.type_line,
    colorIdentity: deriveColorIdentity(catalogCardToView(card)),
    manaCost: card.mana_cost,
    power: card.power,
    toughness: card.toughness,
    keywords: card.keywords,
    landGlyph: basicLandGlyph(card.type_line),
    hasActivatedAbility: hasActivatedAbilityText(card.rules_text),
    artKey: artKeyFor(card.functional_id),
  };
}

/**
 * A minimal display description for a deck row whose identity the catalog does not
 * carry (an older/newer catalog, or a starter-seeded id) — its raw identity stands
 * in as the name so the card never silently vanishes from the running list. Frameless
 * colorless body; no characteristic is invented.
 */
export function fallbackDisplayData(id: string, name: string): CardDisplayData {
  return { name: name || id, typeLine: '', colorIdentity: 'C' };
}
