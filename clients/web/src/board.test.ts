import { describe, expect, it } from 'vitest'

import { boardRows, fieldRows, piles, rowOf } from './board'
import type { CardType } from './protocol'

const perm = (name: string, ...types: CardType[]) => ({ name, types })
const typesOf = (entry: { types: readonly CardType[] }) => entry.types

describe('which row a permanent is drawn in', () => {
  it('puts creatures with the creatures and lands with the lands', () => {
    expect(rowOf(['creature'])).toBe('creatures')
    expect(rowOf(['land'])).toBe('lands')
  })

  it('draws a creature-land with the creatures', () => {
    // A permanent that is a creature is scanned for as a creature: it attacks, blocks, and
    // dies. This is a decision about *where to draw*, not about what the card is — the server
    // said it is both, and it stays both.
    expect(rowOf(['land', 'creature'])).toBe('creatures')
  })

  it('draws everything that is not a creature in the back row', () => {
    // A row of its own cost the two rows a game is played in a fifth of the board each, and
    // drew a dividing line for nothing on the common board that has none of these.
    expect(rowOf(['artifact'])).toBe('lands')
    expect(rowOf(['enchantment'])).toBe('lands')
    expect(rowOf(['planeswalker'])).toBe('lands')
  })

  it('draws a permanent the server stated no types for rather than hiding it', () => {
    // An absent list is "not stated", never "no types". A board that dropped it would be
    // hiding an object the game contains.
    expect(rowOf([])).toBe('lands')
  })
})

describe('arranging a battlefield', () => {
  const board = [
    perm('Forest', 'land'),
    perm('Grizzly Bears', 'creature'),
    perm('Marauder’s Axe', 'artifact'),
    perm('Mountain', 'land'),
    perm('Dryad Arbor', 'land', 'creature'),
  ]

  it('draws creatures nearest the middle of the table', () => {
    const rows = boardRows(board, typesOf)
    expect(rows.map((group) => group.row)).toEqual(['creatures', 'lands'])
  })

  it('mirrors the order for the seat across the table', () => {
    // So the two sets of creatures face each other across the dividing line and combat reads
    // as one band rather than two lists that happen to be stacked.
    const rows = boardRows(board, typesOf, { mirrored: true })
    expect(rows.map((group) => group.row)).toEqual(['lands', 'creatures'])
  })

  it('keeps the order the server listed, inside a row', () => {
    // A board is never re-sorted client-side: the server's enumeration is the only order
    // anything here is entitled to.
    const [creatures] = boardRows(board, typesOf)
    expect(creatures?.entries.map((entry) => entry.name)).toEqual(['Grizzly Bears', 'Dryad Arbor'])
  })

  it('places every permanent exactly once', () => {
    const drawn = boardRows(board, typesOf).flatMap((group) => group.entries)
    expect(drawn).toHaveLength(board.length)
    expect(new Set(drawn.map((entry) => entry.name)).size).toBe(board.length)
  })

  it('omits a row with nothing in it rather than reserving space', () => {
    const rows = boardRows([perm('Forest', 'land')], typesOf)
    expect(rows.map((group) => group.row)).toEqual(['lands'])
  })

  it('is empty for an empty battlefield', () => {
    expect(boardRows([], typesOf)).toEqual([])
  })
})

describe('the rows a field draws', () => {
  const typed = (types: CardType[]) => ({ name: 'x', types })

  it('draws creatures and lands on an empty board', () => {
    // §5: a region's height is a function of the viewport alone. A row that appeared with the
    // first creature would resize every land under it.
    expect(fieldRows([], typesOf).map((row) => row.row)).toEqual(['creatures', 'lands'])
  })

  it('keeps both rows when only one of them has anything in it', () => {
    const rows = fieldRows([typed(['land'])], typesOf)
    expect(rows.map((row) => row.row)).toEqual(['creatures', 'lands'])
    expect(rows[0]?.entries).toEqual([])
  })

  it('never grows a third row: the same two boxes whatever is in the game', () => {
    // The one thing on the board whose *presence* could change a box no longer can, so the
    // layout a player learns on their first turn is the one they have on their twentieth.
    expect(fieldRows([typed(['artifact'])], typesOf).map((row) => row.row)).toEqual([
      'creatures',
      'lands',
    ])
  })

  it('gives creatures the larger share, and the same share whatever is in the row', () => {
    const empty = fieldRows([], typesOf)
    const full = fieldRows([typed(['creature']), typed(['creature'])], typesOf)
    expect(empty.map((row) => row.share)).toEqual(full.map((row) => row.share))
    expect(empty[0]?.share).toBeGreaterThan(empty[1]?.share ?? 0)
  })

  it('mirrors for the seat across the table, so both sets of creatures meet in the middle', () => {
    expect(fieldRows([], typesOf, { mirrored: true }).map((row) => row.row)).toEqual([
      'lands',
      'creatures',
    ])
  })

  it('keeps the server’s order inside a row', () => {
    const rows = fieldRows(
      [
        { name: 'second', types: ['creature'] as CardType[] },
        { name: 'first', types: ['creature'] as CardType[] },
      ],
      typesOf,
    )
    expect(rows[0]?.entries.map((entry) => entry.name)).toEqual(['second', 'first'])
  })
})

describe('the piles a row draws', () => {
  const at = (key: string | undefined, id: string) => ({ key, id })
  const keyOf = (entry: { key: string | undefined }) => entry.key

  it('gathers a run of the same thing into one pile', () => {
    const drawn = piles([at('forest', 'a'), at('forest', 'b'), at('forest', 'c')], keyOf)
    expect(drawn).toHaveLength(1)
    expect(drawn[0]?.entries.map((entry) => entry.id)).toEqual(['a', 'b', 'c'])
  })

  it('breaks a pile the moment anything about a permanent differs', () => {
    // The key is the caller's, and it carries every mark the board draws — so a tapped Forest,
    // an attacking creature, or one carrying a counter never hides behind an identical card.
    const drawn = piles([at('forest', 'a'), at('forest:tapped', 'b'), at('forest', 'c')], keyOf)
    expect(drawn.map((pile) => pile.entries.map((entry) => entry.id))).toEqual([
      ['a'],
      ['b'],
      ['c'],
    ])
  })

  it('never stacks a permanent with no identity to be the same card of', () => {
    // A token (CR 111) has no `functional_id`, so two of them are not known to be one card.
    const drawn = piles([at(undefined, 'a'), at(undefined, 'b')], keyOf)
    expect(drawn).toHaveLength(2)
  })

  it('only gathers a consecutive run, because the order is the server’s', () => {
    const drawn = piles([at('forest', 'a'), at('bear', 'b'), at('forest', 'c')], keyOf)
    expect(drawn.map((pile) => pile.entries.map((entry) => entry.id))).toEqual([
      ['a'],
      ['b'],
      ['c'],
    ])
  })

  it('places every permanent exactly once', () => {
    const entries = [at('forest', 'a'), at('forest', 'b'), at('bear', 'c')]
    expect(piles(entries, keyOf).flatMap((pile) => pile.entries)).toEqual(entries)
  })
})
