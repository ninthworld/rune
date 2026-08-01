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
