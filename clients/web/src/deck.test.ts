/**
 * The draft, the catalog, and the line the deck builder is not allowed to cross.
 *
 * Half of these are round trips — a decklist collapses to counts and expands back to the flat
 * list the wire wants, and losing a copy in either direction is a deck the player did not build.
 * The other half pin the boundary: the rules strip quotes published numbers, the size note is
 * advice about a count, and nothing here ever answers "is this legal".
 */
import { readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { describe, expect, it } from 'vitest'

import {
  EMPTY_CATALOG,
  cardName,
  collect,
  copiesOf,
  deckRules,
  deckSize,
  expand,
  findCards,
  formatOf,
  readCatalog,
  rejectionText,
  seatRange,
  sizeAdvice,
  EMPTY_DECK,
  withCard,
  withCommander,
  withoutCard,
} from './deck'
import { CatalogView } from './protocol'

const FIXTURES = join(
  dirname(fileURLToPath(import.meta.url)),
  '../../../crates/sage-protocol/fixtures',
)

const catalog = readCatalog(
  CatalogView.parse(JSON.parse(readFileSync(join(FIXTURES, 'catalogview.json'), 'utf8'))),
)

describe('a deck under construction', () => {
  it('collapses a decklist to counts and expands back to the same list', () => {
    const cards = ['forest', 'forest', 'serra_angel', 'forest', 'llanowar_elves']
    const draft = collect(cards)
    // First-appearance order, so a deck reads in the order the player built it.
    expect(draft.entries).toEqual([
      { identity: 'forest', count: 3 },
      { identity: 'serra_angel', count: 1 },
      { identity: 'llanowar_elves', count: 1 },
    ])
    expect(deckSize(draft)).toBe(5)
    // Expanding groups the copies, which the wire does not care about — the multiset is what
    // `submit_deck` carries, and it must survive the trip intact.
    expect([...expand(draft)].sort()).toEqual([...cards].sort())
  })

  it('adds and removes one copy at a time', () => {
    let draft = collect(['forest'])
    draft = withCard(draft, 'forest')
    draft = withCard(draft, 'serra_angel')
    expect(copiesOf(draft, 'forest')).toBe(2)
    expect(copiesOf(draft, 'serra_angel')).toBe(1)
    expect(copiesOf(draft, 'llanowar_elves')).toBe(0)

    draft = withoutCard(draft, 'forest')
    expect(copiesOf(draft, 'forest')).toBe(1)
    draft = withoutCard(draft, 'forest')
    // The entry leaves with its last copy rather than lingering at zero.
    expect(draft.entries.map((entry) => entry.identity)).toEqual(['serra_angel'])
    // Removing a card the deck does not hold changes nothing.
    expect(withoutCard(draft, 'forest')).toEqual(draft)
  })

  it('never mutates the draft it was handed', () => {
    const before = collect(['forest'])
    withCard(before, 'forest')
    withoutCard(before, 'forest')
    withCommander(before, 'forest')
    expect(before).toEqual({ entries: [{ identity: 'forest', count: 1 }] })
  })

  it('drops a designation when the designated card leaves the deck', () => {
    let draft = withCommander(collect(['lathliss', 'mountain']), 'lathliss')
    expect(draft.commander).toBe('lathliss')
    draft = withoutCard(draft, 'mountain')
    // Removing another card leaves the designation alone.
    expect(draft.commander).toBe('lathliss')
    draft = withoutCard(draft, 'lathliss')
    // The designation is a pointer into the list; the card left, so the pointer did too. This
    // is bookkeeping over the draft, not a ruling — whether a designation is *legal* is the
    // server's answer, and it is given by rejecting the submission.
    expect(draft.commander).toBeUndefined()
    expect(withCommander(draft, undefined).commander).toBeUndefined()
  })

  it('carries a commander through a collected decklist', () => {
    expect(collect(['a', 'b'], 'a')).toEqual({
      entries: [
        { identity: 'a', count: 1 },
        { identity: 'b', count: 1 },
      ],
      commander: 'a',
    })
    expect(collect([])).toEqual({ entries: [] })
  })
})

describe('the catalog a deck is built from', () => {
  it('indexes the cards, the keywords, the formats, and the AI kinds', () => {
    expect(catalog.cards).toHaveLength(3)
    expect(catalog.byId.get('serra_angel')?.name).toBe('Serra Angel')
    // Deduplicated and sorted across the whole pool, so a filter lists each keyword once.
    expect(catalog.keywords).toEqual(['flying', 'vigilance'])
    // Sorted by id rather than left in wire order: the protocol promises none, and a server
    // that builds the list from a map would otherwise reorder the create-table form per restart.
    expect(catalog.formats.map((format) => format.game_setup)).toEqual(['commander', 'starter-1v1'])
    expect(catalog.aiNames).toEqual({ random: 'Practice bot' })
  })

  it('reads a client that has fetched no catalog as an empty one', () => {
    expect(readCatalog(undefined)).toEqual(EMPTY_CATALOG)
    expect(readCatalog({ catalog_version: 1 })).toEqual(EMPTY_CATALOG)
    expect(formatOf(EMPTY_CATALOG, 'starter-1v1')).toBeUndefined()
    // A name that cannot be resolved falls back to the identity rather than to nothing.
    expect(cardName(EMPTY_CATALOG, 'serra_angel')).toBe('serra_angel')
    expect(cardName(catalog, 'serra_angel')).toBe('Serra Angel')
  })

  it('searches the strings the server sent, case-insensitively', () => {
    const ids = (query: Parameters<typeof findCards>[1]) =>
      findCards(catalog, query).map((card) => card.functional_id)

    expect(ids({})).toEqual(['llanowar_elves', 'serra_angel', 'forest'])
    expect(ids({ text: 'ANGEL' })).toEqual(['serra_angel'])
    // Type line and rules text are searched as text. Finding the word "Land" is not the same as
    // concluding what a land is — nothing downstream is told this card is one.
    expect(ids({ text: 'basic land' })).toEqual(['forest'])
    expect(ids({ text: 'add {g}' })).toEqual(['llanowar_elves', 'forest'])
    expect(ids({ text: '{3}{W}{W}' })).toEqual(['serra_angel'])
    expect(ids({ text: 'nothing here' })).toEqual([])
  })

  it('filters on a stated keyword, and combines it with the text', () => {
    const ids = (query: Parameters<typeof findCards>[1]) =>
      findCards(catalog, query).map((card) => card.functional_id)
    expect(ids({ keyword: 'flying' })).toEqual(['serra_angel'])
    expect(ids({ keyword: 'flying', text: 'forest' })).toEqual([])
    expect(ids({ text: '   ' })).toHaveLength(3)
  })
})

describe('what the format published', () => {
  const starter = formatOf(catalog, 'starter-1v1')
  const commander = formatOf(catalog, 'commander')

  it('quotes the bounds it stated and stays silent about the ones it did not', () => {
    expect(deckRules(starter)).toEqual([
      'At least 40 cards',
      'At most 4 copies of a card, basic lands exempt',
    ])
    // The seat range is a fact about the table, not the deck, so it is asked for separately.
    expect(seatRange(starter)).toBe('2 seats')
    expect(seatRange(commander)).toBe('2–4 seats')
    expect(seatRange(undefined)).toBeUndefined()
    // No `max_deck_size` on the wire means the format published no maximum, and a permissive
    // bound is left unsaid rather than turned into a number nobody sent.
    expect(deckRules(starter).some((rule) => rule.includes('At most 40'))).toBe(false)
  })

  it('says what a commander format requires, in its own singular', () => {
    expect(deckRules(commander)).toEqual([
      'At least 100 cards',
      'At most 100 cards',
      'At most 1 copy of a card, basic lands exempt',
      'A commander must be designated',
      "Every card's color identity must fit the commander's",
    ])
  })

  it('has nothing to say about a format the catalog never described', () => {
    expect(deckRules(undefined)).toEqual([])
    expect(formatOf(catalog, 'no-such-format')).toBeUndefined()
  })
})

describe('advice, and the verdict that is not ours', () => {
  const starter = formatOf(catalog, 'starter-1v1')
  const commander = formatOf(catalog, 'commander')
  const deckOf = (size: number) => collect(Array.from({ length: size }, (_, i) => `card_${i % 7}`))

  it('counts a deck against the size the format published, in both directions', () => {
    expect(sizeAdvice(deckOf(37), starter)).toBe('3 short of the 40-card minimum')
    expect(sizeAdvice(deckOf(40), starter)).toBeUndefined()
    // No maximum published, so a deck can be any size above the floor without a word said.
    expect(sizeAdvice(deckOf(250), starter)).toBeUndefined()
    expect(sizeAdvice(deckOf(101), commander)).toBe('1 above the 100-card maximum')
  })

  it('says nothing at all about a format it was not given', () => {
    // Size is the one deck rule that needs only counting. Copies and color identity depend on
    // what a card *is*, and that answer is the server's — so there is no advice about them.
    expect(sizeAdvice(deckOf(1), undefined)).toBeUndefined()
    expect(sizeAdvice(EMPTY_DECK, undefined)).toBeUndefined()
  })

  it('shows the server’s rejection verbatim, naming the card it named', () => {
    expect(
      rejectionText(
        { code: 'below_minimum', reason: 'deck has 39 cards, below the 40-card minimum' },
        catalog,
      ),
    ).toBe('deck has 39 cards, below the 40-card minimum')
    // The identity on the wire is resolved to the name a player reads; the reason is untouched.
    expect(
      rejectionText(
        { code: 'copy_limit', reason: 'above the 4-copy limit', card: 'serra_angel' },
        catalog,
      ),
    ).toBe('above the 4-copy limit (Serra Angel)')
    // A card the catalog does not list still gets named, as the identity the server sent.
    expect(
      rejectionText({ code: 'unknown_card', reason: 'not a card', card: 'made_up' }, catalog),
    ).toBe('not a card (made_up)')
  })
})
