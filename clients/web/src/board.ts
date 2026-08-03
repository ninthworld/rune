/**
 * A battlefield, arranged into rows.
 *
 * A board is read by *where things are* long before it is read by text, and one wrapping list of
 * permanents in whatever order the server enumerated them denies a player that entirely: the
 * land they are counting sits between the two creatures they are comparing. So permanents are
 * grouped — creatures, lands, and everything else — and the grouping happens once, here.
 *
 * **The client is not deciding what a card is.** `card_types` is stated by the server, from the
 * same projection the type line is rendered from (`docs/protocol.md`), precisely so no surface
 * has to parse `"Artifact Creature — Thopter"` to find out. What this module decides is the
 * other question — *where to draw* a permanent that is more than one thing — and that is
 * presentation with no rules content at all. A creature-land is a creature and a land no matter
 * which row it is in; the row is only about which of its two natures a player is scanning for.
 *
 * Rows mirror across the table. Your creatures sit nearest the middle and your lands furthest
 * from it; the opponent's are reversed, so the two sets of creatures face each other across the
 * dividing line and combat reads as one band instead of two lists that happen to be stacked.
 */
import type { CardType } from './protocol'

export type BoardRow = 'creatures' | 'other' | 'lands'

/**
 * Which row one permanent is drawn in.
 *
 * Precedence, and the reason for it: a permanent that is a creature is scanned for as a
 * creature — it attacks, it blocks, it dies — so an animated land and a creature-land are drawn
 * with the creatures. Everything else that is a land is drawn as mana. A permanent whose types
 * the server did not state is not a mystery to be solved; it goes in `other` and renders
 * normally, because an absent list is "not stated" and never "no types".
 */
export function rowOf(types: readonly CardType[]): BoardRow {
  if (types.includes('creature')) return 'creatures'
  if (types.includes('land')) return 'lands'
  return 'other'
}

/** One row of a battlefield, with the wording a screen reader gets for it. */
export interface BoardGroup<T> {
  row: BoardRow
  label: string
  entries: readonly T[]
}

const LABELS: Record<BoardRow, string> = {
  creatures: 'Creatures',
  other: 'Other permanents',
  lands: 'Lands',
}

/** Nearest the middle of the table first. The opponent's half draws this reversed. */
const ORDER: readonly BoardRow[] = ['creatures', 'other', 'lands']

/**
 * Group permanents into the rows a battlefield draws.
 *
 * Within a row, permanents stay in the order the server listed them — a board is not re-sorted
 * client-side, because the order the server enumerated is the only order anything here is
 * entitled to. An empty row is omitted rather than reserving space a board does not have.
 */
export function boardRows<T>(
  entries: readonly T[],
  typesOf: (entry: T) => readonly CardType[],
  { mirrored = false }: { mirrored?: boolean } = {},
): readonly BoardGroup<T>[] {
  const order = mirrored ? [...ORDER].reverse() : ORDER
  return order.flatMap((row) => {
    const inRow = entries.filter((entry) => rowOf(typesOf(entry)) === row)
    return inRow.length === 0 ? [] : [{ row, label: LABELS[row], entries: inRow }]
  })
}

/**
 * The rows a *field* draws, which is not the same question as which rows have anything in them.
 *
 * `docs/client-design.md` §5: **a region's height is a function of the viewport alone.** A row
 * that appeared when the first creature was cast would resize every land under it, and the
 * layout a player learns on their first turn is the one they have on their twentieth. So
 * creatures and lands are drawn whether or not anything is in them, and they keep their share of
 * the field's height either way.
 *
 * The third row is the exception the same rule allows: it appears when the server states a
 * *kind* of permanent that is neither — an artifact, an enchantment, a planeswalker — which is a
 * fact about what is in the game rather than a count of what is in a row. It is the only thing
 * on the board whose presence can change a box, and it changes it at most once.
 *
 * `share` is the fraction of the field's height, as a grid `fr`. Creatures take the larger one,
 * because that is the row a game is played in.
 */
export interface FieldRow<T> {
  row: BoardRow
  label: string
  share: number
  entries: readonly T[]
}

const SHARE: Record<BoardRow, number> = { creatures: 3, other: 2, lands: 2 }
const ALWAYS: readonly BoardRow[] = ['creatures', 'lands']

export function fieldRows<T>(
  entries: readonly T[],
  typesOf: (entry: T) => readonly CardType[],
  { mirrored = false }: { mirrored?: boolean } = {},
): readonly FieldRow<T>[] {
  const rows = ORDER.filter(
    (row) => ALWAYS.includes(row) || entries.some((entry) => rowOf(typesOf(entry)) === row),
  ).map((row) => ({
    row,
    label: LABELS[row],
    share: SHARE[row],
    entries: entries.filter((entry) => rowOf(typesOf(entry)) === row),
  }))
  return mirrored ? [...rows].reverse() : rows
}
