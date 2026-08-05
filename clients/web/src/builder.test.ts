/**
 * The columns the builder cuts its draft into.
 *
 * What is worth pinning is that a column is a reading of what the server sent — the printed
 * cost added up, the identity it carried, the types it carried — and that every copy in the
 * draft ends up in exactly one of them.
 */
import { describe, expect, it } from 'vitest'

import { CARD_KINDS, deckColumns, deckStats, kindsOf, manaValue, poolCards } from './builder'
import { collect, readCatalog } from './deck'
import type { CatalogCard } from './protocol'

const card = (
  over: Partial<CatalogCard> & { functional_id: string; name: string },
): CatalogCard => ({
  type_line: 'Creature',
  ...over,
})

const CATALOG = readCatalog({
  catalog_version: 1,
  cards: [
    card({
      functional_id: 'bear',
      name: 'Bear',
      mana_cost: '{1}{G}',
      card_types: ['creature'],
      color_identity: ['G'],
    }),
    card({
      functional_id: 'bolt',
      name: 'Bolt',
      mana_cost: '{R}',
      card_types: ['instant'],
      color_identity: ['R'],
    }),
    card({
      functional_id: 'forest',
      name: 'Forest',
      card_types: ['land'],
    }),
    card({
      functional_id: 'gold',
      name: 'Gold Thing',
      mana_cost: '{4}{G}{R}',
      card_types: ['creature'],
      color_identity: ['R', 'G'],
    }),
  ],
})

const DRAFT = collect(['bear', 'bear', 'bolt', 'forest', 'forest', 'forest', 'gold'])

describe('manaValue', () => {
  it('adds the generic up and counts every other pip as one', () => {
    expect(manaValue('{4}{G}{R}')).toBe(6)
    expect(manaValue('{R}')).toBe(1)
    expect(manaValue(undefined)).toBe(0)
  })

  it('counts {X} as nothing, because nothing is what it is printed as', () => {
    expect(manaValue('{X}{R}')).toBe(1)
  })
})

describe('poolCards', () => {
  const names = (kinds: readonly (typeof CARD_KINDS)[number][], text = '') =>
    poolCards(CATALOG, { text, kinds }).map((card) => card.name)

  it('shows the whole catalog while every kind is on', () => {
    expect(names(CARD_KINDS)).toEqual(['Bear', 'Bolt', 'Forest', 'Gold Thing'])
  })

  it('drops a kind the player switched off', () => {
    expect(names(['W', 'U', 'B', 'R', 'G', 'C'])).not.toContain('Forest')
    expect(names(['land'])).toEqual(['Forest'])
  })

  it('keeps a gold card while any one of its colours is on', () => {
    expect(names(['G'])).toEqual(['Bear', 'Gold Thing'])
    expect(names(['R'])).toEqual(['Bolt', 'Gold Thing'])
  })

  it('matches the name, and nothing else the card says', () => {
    expect(names(CARD_KINDS, 'bo')).toEqual(['Bolt'])
    expect(names(CARD_KINDS, 'creature')).toEqual([])
  })

  it('a land is a land whatever it costs, and a card with no identity is colourless', () => {
    expect(kindsOf(CATALOG.cards[2]!)).toEqual(['land'])
    expect(kindsOf(CATALOG.cards[1]!)).toEqual(['R'])
  })
})

describe('deckColumns', () => {
  it('holds every copy exactly once, whatever it is cut by', () => {
    for (const sort of ['cost', 'color', 'type'] as const) {
      const total = deckColumns(DRAFT, CATALOG, sort).reduce((sum, c) => sum + c.cards.length, 0)
      expect(total).toBe(7)
    }
  })

  it('cuts by cost, in curve order, with the lands past the end of it', () => {
    const columns = deckColumns(DRAFT, CATALOG, 'cost')
    // A land has no cost, so it is not a nothing-cost spell sitting in the 0 column.
    expect(columns.map((column) => column.label)).toEqual([
      '1 Mana (1 card)',
      '2 Mana (2 cards)',
      '6 Mana (1 card)',
      'Lands (3 cards)',
    ])
  })

  it('cuts by the identity the catalog carried, multicolor and colorless included', () => {
    const columns = deckColumns(DRAFT, CATALOG, 'color')
    expect(columns.map((column) => column.label)).toEqual([
      'Red (1 card)',
      'Green (2 cards)',
      'Multicolor (1 card)',
      'Colorless (3 cards)',
    ])
  })

  it('cuts by the first type the catalog carried', () => {
    const columns = deckColumns(DRAFT, CATALOG, 'type')
    expect(columns.map((column) => column.label)).toEqual([
      'Creature (3 cards)',
      'Instant (1 card)',
      'Land (3 cards)',
    ])
  })

  it('says nothing about a draft holding a card the catalog does not', () => {
    expect(deckColumns(collect(['ghost']), CATALOG, 'cost')).toEqual([])
  })
})

describe('deckStats', () => {
  it('counts the curve at every step, including the ones with nothing on them', () => {
    const stats = deckStats(DRAFT.entries, CATALOG)

    // The three Forests are in the deck but not on the curve: a land is not a free spell.
    expect(stats.total).toBe(7)
    expect(stats.curve.map((step) => `${step.label}:${step.count}`)).toEqual([
      '0:0',
      '1:1',
      '2:2',
      '3:0',
      '4:0',
      '5:0',
      '6:1',
      '7+:0',
    ])
  })

  it('counts a gold card once for each of its colours, and says so in the numbers', () => {
    const stats = deckStats(DRAFT.entries, CATALOG)

    // Bolt is red; the gold card is both, so it is in each column.
    expect(stats.colors.map((slice) => `${slice.label}:${slice.count}`)).toEqual([
      'Red:2',
      'Green:3',
      'Colorless:3',
    ])
  })

  it('counts types by the first the catalog stated, most first', () => {
    const stats = deckStats(DRAFT.entries, CATALOG)

    expect(stats.types.map((type) => `${type.label}:${type.count}`)).toEqual([
      'Creature:3',
      'Land:3',
      'Instant:1',
    ])
  })

  it('says nothing about an empty deck rather than dividing by it', () => {
    const stats = deckStats([], CATALOG)
    expect(stats.total).toBe(0)
    expect(stats.colors).toEqual([])
    expect(stats.curve.every((step) => step.count === 0)).toBe(true)
  })
})
