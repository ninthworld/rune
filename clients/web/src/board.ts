/**
 * A battlefield, arranged into rows, and the piles a row draws.
 *
 * A board is read by *where things are* long before it is read by text, and one wrapping list of
 * permanents in whatever order the server enumerated them denies a player that entirely: the
 * land they are counting sits between the two creatures they are comparing. So permanents are
 * grouped — creatures, and everything else — and the grouping happens once, here.
 *
 * **The client is not deciding what a card is.** `card_types` is stated by the server, from the
 * same projection the type line is rendered from (`docs/protocol.md`), precisely so no surface
 * has to parse `"Artifact Creature — Thopter"` to find out. What this module decides is the
 * other question — *where to draw* a permanent that is more than one thing — and that is
 * presentation with no rules content at all. A creature-land is a creature and a land no matter
 * which row it is in; the row is only about which of its two natures a player is scanning for.
 *
 * **Two rows, not three.** An artifact, an enchantment and a planeswalker used to get a row of
 * their own, which cost the two rows a game is actually played in a fifth of the board each
 * and, on the common board that has none of them, drew a dividing line for nothing. They sit in
 * the back row with the lands: it is the row of things that sit there, and a player scanning
 * for a blocker is scanning the front row either way.
 *
 * Rows mirror across the table. Your creatures sit nearest the middle and your back row furthest
 * from it; the opponent's are reversed, so the two sets of creatures face each other across the
 * dividing line and combat reads as one band instead of two lists that happen to be stacked.
 */
import type { CardType } from './protocol'

export type BoardRow = 'creatures' | 'lands'

/**
 * Which row one permanent is drawn in.
 *
 * Precedence, and the reason for it: a permanent that is a creature is scanned for as a
 * creature — it attacks, it blocks, it dies — so an animated land and a creature-land are drawn
 * with the creatures. Everything else is drawn in the back row. A permanent whose types the
 * server did not state is not a mystery to be solved; it goes in the back row and renders
 * normally, because an absent list is "not stated" and never "no types".
 */
export function rowOf(types: readonly CardType[]): BoardRow {
  return types.includes('creature') ? 'creatures' : 'lands'
}

/** One row of a battlefield, with the wording a screen reader gets for it. */
export interface BoardGroup<T> {
  row: BoardRow
  label: string
  entries: readonly T[]
}

const LABELS: Record<BoardRow, string> = {
  creatures: 'Creatures',
  lands: 'Lands and other permanents',
}

/** Nearest the middle of the table first. The opponent's half draws this reversed. */
const ORDER: readonly BoardRow[] = ['creatures', 'lands']

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
 * layout a player learns on their first turn is the one they have on their twentieth. So both
 * rows are drawn whether or not anything is in them, and they keep their share of the field's
 * height either way — which is now unconditional, because there is no longer a row whose
 * *existence* depends on what is in the game.
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

const SHARE: Record<BoardRow, number> = { creatures: 3, lands: 2 }

export function fieldRows<T>(
  entries: readonly T[],
  typesOf: (entry: T) => readonly CardType[],
  { mirrored = false }: { mirrored?: boolean } = {},
): readonly FieldRow<T>[] {
  const rows = ORDER.map((row) => ({
    row,
    label: LABELS[row],
    share: SHARE[row],
    entries: entries.filter((entry) => rowOf(typesOf(entry)) === row),
  }))
  return mirrored ? [...rows].reverse() : rows
}

/**
 * One pile a row draws: consecutive permanents a player has no reason to tell apart.
 *
 * Eight Forests in a row is eight boxes saying the same thing, and on a phone it is the whole
 * width of the board spent on the least interesting half of it. So they are drawn as one
 * overlapping pile — which is what a player does with them on a table — and the pile is decided
 * here rather than in a component, for the same reason the rows are.
 *
 * **Only permanents that are the same in every way a player reads.** The key below is
 * deliberately over-strict: two cards stack when they are the same card, in the same tap state,
 * with the same counters, damage, markers, attachments, and the same relationships — anything
 * the board would have drawn differently on one of them breaks the pile. A creature with a
 * +1/+1 counter never hides behind an identical one without it, an attacker never hides behind
 * a creature that is not attacking, and the arrow that has to reach one of them always has a
 * whole card to reach.
 *
 * **Same card** is `artKey` — the server's `functional_id`, the stable identity of the card
 * definition (`card-face.ts`). Never the name: two different cards may share a name across
 * printings, and a token has no identity at all (CR 111), so a token never stacks with anything
 * including another token. That is the safe direction: failing to stack costs width, stacking
 * two things that are not the same costs a player the board.
 *
 * **Consecutive only.** The server's enumeration order is the only order this client is entitled
 * to (`boardRows`), so a pile is a run in that order and never a re-sort that would move a
 * permanent away from where its owner last saw it.
 */
export interface Pile<T> {
  /** What the entries have in common, or `undefined` where nothing may be stacked. */
  key: string | undefined
  entries: readonly T[]
}

export function piles<T>(
  entries: readonly T[],
  keyOf: (entry: T) => string | undefined,
): readonly Pile<T>[] {
  const out: Pile<T>[] = []
  for (const entry of entries) {
    const key = keyOf(entry)
    const last = out[out.length - 1]
    if (key !== undefined && last && last.key === key) {
      out[out.length - 1] = { key, entries: [...last.entries, entry] }
    } else {
      out.push({ key, entries: [entry] })
    }
  }
  return out
}
