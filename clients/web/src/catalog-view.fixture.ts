/**
 * A representative server→client `CatalogView` frame (issue #367) for the deck-builder
 * suite (issue #368), mirroring the round-trip fixtures in the wire tests. Written as
 * raw wire JSON — elided optionals (a basic land's absent cost/P-T, a permissive
 * format's `None` upper bounds) are omitted exactly as the server omits them, so tests
 * exercise the client's normalization.
 */
import type { CatalogView } from './protocol';
import { normalizeCatalogView } from './wire';

/** Raw wire catalog: two spells with full characteristics and one basic land. */
export const CATALOG_JSON = JSON.stringify({
  catalog_version: 1,
  cards: [
    {
      functional_id: 'serra_angel',
      name: 'Serra Angel',
      type_line: 'Creature — Angel',
      mana_cost: '{3}{W}{W}',
      rules_text: 'Flying, vigilance',
      power: '4',
      toughness: '4',
      keywords: ['flying', 'vigilance'],
    },
    {
      functional_id: 'shock',
      name: 'Shock',
      type_line: 'Instant',
      mana_cost: '{R}',
      rules_text: 'Shock deals 2 damage to any target.',
    },
    {
      functional_id: 'forest',
      name: 'Forest',
      type_line: 'Basic Land — Forest',
      rules_text: '{T}: Add {G}.',
    },
    {
      functional_id: 'arcades_the_strategist',
      name: 'Arcades the Strategist',
      type_line: 'Legendary Creature — Elder Dragon',
      mana_cost: '{2}{G}{W}{U}',
      rules_text: 'Flying, vigilance',
      power: '3',
      toughness: '5',
      keywords: ['flying', 'vigilance'],
    },
  ],
  formats: [
    {
      game_setup: '1v1',
      min_deck_size: 40,
      max_copies: 4,
      basic_land_exempt: true,
      min_seats: 2,
      max_seats: 2,
    },
    {
      game_setup: 'ffa-4',
      min_deck_size: 0,
      basic_land_exempt: true,
      min_seats: 2,
      max_seats: 8,
    },
    {
      // A commander format (issue #394/#396): advertises the designation requirement so
      // a client learns it from metadata rather than hardcoding the format name.
      game_setup: 'commander',
      min_deck_size: 100,
      max_deck_size: 100,
      max_copies: 1,
      basic_land_exempt: true,
      requires_commander: true,
      enforce_color_identity: true,
      min_seats: 2,
      max_seats: 4,
    },
  ],
});

/** The typed, normalized form of {@link CATALOG_JSON}. */
export const CATALOG_VIEW: CatalogView = normalizeCatalogView(JSON.parse(CATALOG_JSON));
