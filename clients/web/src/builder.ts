/**
 * The deck builder's reading of its own draft: what the columns are, and what is in them.
 *
 * **This states data the server sent; it decides no rule.** Mana value is arithmetic on the
 * printed cost, the colours are the identity the catalog carried, and the types are the
 * `card_types` it carried — the same shallow reading the card frame does. Nothing here knows
 * whether a deck is legal.
 */
import { manaSymbols } from './mana'
import type { CatalogCard } from './protocol'
import type { Catalog, DeckDraft, DeckEntry } from './deck'

/** How much of each card in a column is shown. */
export type DeckView = 'full' | 'stacked' | 'titles'

/** What the columns are cut by. */
export type DeckSort = 'cost' | 'color' | 'type'

/** What the search's toggles cut the catalog by: the five colours, colourless, and lands. */
export const CARD_KINDS = ['W', 'U', 'B', 'R', 'G', 'C', 'land'] as const
export type CardKind = (typeof CARD_KINDS)[number]

export interface PoolQuery {
  /** Matched case-insensitively against the name the catalog stated. */
  text: string
  /** The kinds still switched on; a kind switched off is one the player does not want to see. */
  kinds: readonly CardKind[]
}

export interface DeckColumn {
  /** What every card in this column has in common, and how many of them there are. */
  label: string
  /** One entry per copy, so a column's length is how many cards are in it. */
  cards: readonly CatalogCard[]
}

/**
 * The printed cost added up: generic pips for what they say, every other pip for one, `{X}` for
 * nothing — the ordinary reading of a mana value, over a cost the server printed.
 */
export function manaValue(cost: string | undefined): number {
  let total = 0
  for (const symbol of manaSymbols(cost)) {
    if (symbol.kind === 'variable' || symbol.kind === 'other') continue
    total += symbol.kind === 'generic' ? Number(symbol.glyph) : 1
  }
  return total
}

const COLOR_NAMES: Record<string, string> = {
  W: 'White',
  U: 'Blue',
  B: 'Black',
  R: 'Red',
  G: 'Green',
}
const COLOR_ORDER = ['White', 'Blue', 'Black', 'Red', 'Green', 'Multicolor', 'Colorless']

/** The wire's types, in the order a builder reads them; the label is the same word set upright. */
const TYPE_ORDER = [
  'creature',
  'planeswalker',
  'instant',
  'sorcery',
  'artifact',
  'enchantment',
  'battle',
  'land',
]

const upright = (word: string): string => word.charAt(0).toUpperCase() + word.slice(1)

/** Costs beyond this share one column, because a curve's tail is one bucket. */
const COST_CAP = 7

/** The one type read anywhere but as a label: a land has no cost, rather than a cost of nothing. */
const isLand = (card: CatalogCard): boolean => card.card_types?.includes('land') === true

/** What the column is: the mana value as a number, a colour's name, or a type's. */
function groupOf(card: CatalogCard, sort: DeckSort): string {
  if (sort === 'cost') {
    if (isLand(card)) return 'Lands'
    const value = manaValue(card.mana_cost)
    return value >= COST_CAP ? `${COST_CAP}+` : String(value)
  }
  if (sort === 'color') {
    const identity = card.color_identity ?? []
    if (identity.length > 1) return 'Multicolor'
    const first = identity[0]
    return (first && COLOR_NAMES[first.toUpperCase()]) ?? 'Colorless'
  }
  return upright(card.card_types?.[0] ?? 'other')
}

/** What the column says over itself: what it holds, and how many cards that is. */
function headingOf(group: string, sort: DeckSort, count: number): string {
  const what = sort === 'cost' && group !== 'Lands' ? `${group} Mana` : group
  return `${what} (${count} ${count === 1 ? 'card' : 'cards'})`
}

function rank(group: string, sort: DeckSort): number {
  // The lands sit past the far end of the curve, which is where a decklist puts them.
  if (sort === 'cost') return group === 'Lands' ? COST_CAP + 1 : Number(group.replace('+', ''))
  const order = sort === 'color' ? COLOR_ORDER : TYPE_ORDER.map(upright)
  const at = order.indexOf(group)
  return at === -1 ? order.length : at
}

/** One bar, slice, or row of a deck's summary: what it is, and how many cards. */
export interface DeckTally {
  label: string
  /** The key the surface paints it by — a colour letter, a type, or a mana value. */
  key: string
  count: number
}

/** What a deck is made of, as three readings of the same list. */
export interface DeckStats {
  total: number
  /** How many cards at each mana value, `0` through `7+`, with every step in between. */
  curve: readonly DeckTally[]
  /**
   * Cards per colour of the identity the catalog stated, plus the colourless ones.
   *
   * A card counts once for **each** colour it is in, so a gold card is in both its columns and
   * the slices add up to more than the deck. That is the reading a builder wants — "how much
   * green am I asking for" — and it is why the slices carry counts rather than percentages.
   */
  colors: readonly DeckTally[]
  /** Cards per first type the catalog stated. */
  types: readonly DeckTally[]
}

const COLOR_LETTERS = ['W', 'U', 'B', 'R', 'G'] as const

const tally = (counts: Map<string, number>, label: (key: string) => string): DeckTally[] =>
  [...counts].map(([key, count]) => ({ key, label: label(key), count }))

/**
 * A deck summarised three ways: its curve, its colours, and its types.
 *
 * **Every number is a count of cards the server described.** Nothing here is a verdict about a
 * deck — a curve with nothing above four is not "wrong", it is what the deck is.
 */
export function deckStats(entries: readonly DeckEntry[], catalog: Catalog): DeckStats {
  const curve = new Map<string, number>()
  const colors = new Map<string, number>()
  const types = new Map<string, number>()
  let total = 0

  for (let step = 0; step <= COST_CAP; step += 1) curve.set(String(step), 0)

  for (const entry of entries) {
    const card = catalog.byId.get(entry.identity)
    if (!card) continue
    total += entry.count

    // A land is not a nothing-cost spell — it is not on the curve at all. Counting it as zero
    // would put the deck's whole land base in the column a builder reads as "free spells".
    if (!isLand(card)) {
      const value = Math.min(manaValue(card.mana_cost), COST_CAP)
      curve.set(String(value), (curve.get(String(value)) ?? 0) + entry.count)
    }

    const identity = (card.color_identity ?? []).map((color) => color.toUpperCase())
    for (const key of identity.length > 0 ? identity : ['C'])
      colors.set(key, (colors.get(key) ?? 0) + entry.count)

    const type = card.card_types?.[0] ?? 'other'
    types.set(type, (types.get(type) ?? 0) + entry.count)
  }

  return {
    total,
    curve: tally(curve, (key) => (Number(key) >= COST_CAP ? `${COST_CAP}+` : key)),
    colors: tally(colors, (key) => COLOR_NAMES[key] ?? 'Colorless').sort(
      (a, b) =>
        [...COLOR_LETTERS, 'C'].indexOf(a.key as 'W') -
        [...COLOR_LETTERS, 'C'].indexOf(b.key as 'W'),
    ),
    types: tally(types, upright).sort(
      (a, b) => b.count - a.count || a.label.localeCompare(b.label),
    ),
  }
}

/**
 * The kinds one card answers to: a land is a land whatever it costs, and everything else is the
 * colours of the identity the catalog carried, or colourless where it carried none.
 */
export function kindsOf(card: CatalogCard): readonly CardKind[] {
  if (card.card_types?.includes('land')) return ['land']
  const identity = (card.color_identity ?? []).map((color) => color.toUpperCase() as CardKind)
  return identity.length > 0 ? identity : ['C']
}

/**
 * The catalog narrowed to what the search is asking for: a name that contains the text, and at
 * least one kind still switched on. A gold card shows while any of its colours is on.
 */
export function poolCards(catalog: Catalog, query: PoolQuery): readonly CatalogCard[] {
  const text = query.text.trim().toLowerCase()
  return catalog.cards.filter(
    (card) =>
      (text === '' || card.name.toLowerCase().includes(text)) &&
      kindsOf(card).some((kind) => query.kinds.includes(kind)),
  )
}

/**
 * The draft cut into columns: one column per value of the chosen key, in that key's own order,
 * each holding one entry per copy sorted by cost then name.
 */
export const deckColumns = (
  draft: DeckDraft,
  catalog: Catalog,
  sort: DeckSort,
): readonly DeckColumn[] => entryColumns(draft.entries, catalog, sort)

/** The same cut, over either of a draft's two lists — the deck, or the cards beside it. */
export function entryColumns(
  entries: readonly DeckEntry[],
  catalog: Catalog,
  sort: DeckSort,
): readonly DeckColumn[] {
  const copies = entries.flatMap((entry) =>
    Array.from({ length: entry.count }, () => entry.identity),
  )
  const columns = new Map<string, CatalogCard[]>()
  for (const identity of copies) {
    const card = catalog.byId.get(identity)
    if (!card) continue
    const group = groupOf(card, sort)
    const cards = columns.get(group)
    if (cards) cards.push(card)
    else columns.set(group, [card])
  }

  return [...columns]
    .sort(([a], [b]) => rank(a, sort) - rank(b, sort) || a.localeCompare(b))
    .map(([group, cards]) => ({
      label: headingOf(group, sort, cards.length),
      cards: [...cards].sort(
        (a, b) => manaValue(a.mana_cost) - manaValue(b.mana_cost) || a.name.localeCompare(b.name),
      ),
    }))
}
