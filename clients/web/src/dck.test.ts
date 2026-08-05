/**
 * Decks as files.
 *
 * What is worth pinning is the join: a file names cards, this client addresses identities, and
 * a name the catalog does not hold has to come back as a name the player can read rather than
 * as a card quietly missing from their deck.
 */
import { describe, expect, it } from 'vitest'

import { formatDck, parseDck, resolveDeck } from './dck'
import { readCatalog, sideEntries } from './deck'
import type { CatalogCard } from './protocol'

const card = (
  functional_id: string,
  name: string,
  over: Partial<CatalogCard> = {},
): CatalogCard => ({
  functional_id,
  name,
  type_line: 'Creature',
  ...over,
})

const CATALOG = readCatalog({
  catalog_version: 1,
  cards: [
    card('lightning_bolt', 'Lightning Bolt', { mana_cost: '{R}' }),
    card('goblin_guide', 'Goblin Guide', { mana_cost: '{R}' }),
    card('tormods_crypt', "Tormod's Crypt"),
    card('krenko_mob_boss', 'Krenko, Mob Boss', { mana_cost: '{2}{R}{R}' }),
  ],
})

const FILE = `[Main]
4 Lightning Bolt|LEA
2 Goblin Guide|ZEN

[Sideboard]
3 Tormod's Crypt|DRK

[Commander]
1 Krenko, Mob Boss|JMP
`

describe('parseDck', () => {
  it('reads the three sections, the counts, and the set beside each name', () => {
    const file = parseDck(FILE)

    expect(file.main).toEqual([
      { count: 4, name: 'Lightning Bolt', set: 'LEA' },
      { count: 2, name: 'Goblin Guide', set: 'ZEN' },
    ])
    expect(file.side).toEqual([{ count: 3, name: "Tormod's Crypt", set: 'DRK' }])
    expect(file.commander).toEqual([{ count: 1, name: 'Krenko, Mob Boss', set: 'JMP' }])
  })

  it('takes a bare list as the deck, and a line with no set as a card all the same', () => {
    expect(parseDck('4 Lightning Bolt\n1 Goblin Guide').main).toEqual([
      { count: 4, name: 'Lightning Bolt' },
      { count: 1, name: 'Goblin Guide' },
    ])
  })

  it('skips what it cannot read rather than losing the file over one line', () => {
    const file = parseDck('[Main]\n// a comment\nnonsense\n\n2 Goblin Guide')
    expect(file.main).toEqual([{ count: 2, name: 'Goblin Guide' }])
  })
})

describe('resolveDeck', () => {
  it('joins names to identities and carries the commander into the deck as well', () => {
    const { draft, missing } = resolveDeck(parseDck(FILE), CATALOG)

    expect(missing).toEqual([])
    expect(draft.commander).toBe('krenko_mob_boss')
    expect(draft.entries).toEqual([
      { identity: 'krenko_mob_boss', count: 1 },
      { identity: 'lightning_bolt', count: 4 },
      { identity: 'goblin_guide', count: 2 },
    ])
    expect(sideEntries(draft)).toEqual([{ identity: 'tormods_crypt', count: 3 }])
  })

  it('matches a name whatever case it was written in, and ignores the set code', () => {
    const { draft, missing } = resolveDeck(parseDck('4 lightning bolt|4ED'), CATALOG)
    expect(missing).toEqual([])
    expect(draft.entries).toEqual([{ identity: 'lightning_bolt', count: 4 }])
  })

  it('names what it could not find, once each, and keeps the rest of the deck', () => {
    const { draft, missing } = resolveDeck(
      parseDck('4 Black Lotus\n2 Goblin Guide\n1 Black Lotus\n3 Time Walk'),
      CATALOG,
    )

    expect(missing).toEqual(['Black Lotus', 'Time Walk'])
    expect(draft.entries).toEqual([{ identity: 'goblin_guide', count: 2 }])
  })
})

describe('formatDck', () => {
  it('writes back what it read, without inventing a printing it was never told', () => {
    const { draft } = resolveDeck(parseDck(FILE), CATALOG)

    expect(formatDck(draft, CATALOG)).toBe(
      `[Main]
4 Lightning Bolt
2 Goblin Guide

[Sideboard]
3 Tormod's Crypt

[Commander]
1 Krenko, Mob Boss
`,
    )
  })

  it('round-trips a deck through the file and back', () => {
    const first = resolveDeck(parseDck(FILE), CATALOG).draft
    const again = resolveDeck(parseDck(formatDck(first, CATALOG)), CATALOG).draft

    expect(again).toEqual(first)
  })
})
