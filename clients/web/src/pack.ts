/**
 * Packing a battlefield: how big a permanent is drawn, and where it sits.
 *
 * `scene.ts` answers how much room a field gets; this answers what happens to the permanents
 * inside it. The two halves of `docs/client-design.md` §5 are exactly that split — **regions are
 * sized by the viewport, and counts are absorbed by the cards** — and this module is the second
 * half. Fifteen permanents in a field sized for six do not make the field taller: they make the
 * cards smaller, then overlapped, then chips, and the box never moves.
 *
 * The arithmetic is §5's, over the **turned footprint** rather than the upright width:
 *
 * ```
 * ideal     = the band's designed card height
 * fitted    = (W - (N-1)·g) / N
 * footprint = clamp(FLOOR, fitted, ideal)
 * ```
 *
 * A tapped permanent is a quarter turn (§6), so the room a permanent takes along its row is its
 * own height, and the row charges it **whether or not anything is currently tapped** — a
 * reservation that appeared when a creature attacked would slide every other card on the board,
 * which §5 forbids outright. Height is therefore what a turn costs: a crowded row draws a shorter
 * card than the same room would have drawn upright, and the printed 63:88 takes the width from
 * there.
 *
 * **The floor is soft downward and hard sideways** (§5), and the two directions are not the same
 * problem. Too many cards for the width: the row stops spacing and starts **overlapping**, the
 * pitch becomes `(W - footprint) / (N - 1)`, and the cards keep their size — a card shrunk to fit
 * is unreadable everywhere at once, while an overlapped row is unreadable only where it is
 * covered, and what stays exposed is the top-left strip, which is the name band. A fanned row is
 * read as its names. Too *short* a row for a 100px card: the card simply gets smaller and stays a
 * card. Nothing is given up to hold 72×100, because giving something up is what §3 exists to
 * forbid; the tile stops being a card only where its name band can no longer set a name at §2's
 * 9px floor, which is `fit.ts`'s `CARD_MIN_HEIGHT` and not a number this file carries.
 *
 * One thing comes before the fan, and it is the only addition to §5 here: **a row takes a second
 * line while it has the height for one at the same tile size.** A chip is 30px in a 60px box and a
 * card that has shrunk to its floor is 101px in a field that was 229 — in both, the height going
 * spare buys a line of whole tiles where the fan would have cost every tile after the first its
 * right-hand side. It is never bought with the tile: a line paid for by shorter cards would be
 * §3's step 6 taken early, and the card is what the ladder protects longest.
 *
 * Two things the row height decides, and neither is a judgment per screen. **A permanent is a card
 * while its row can still set a name**; below that it is a landscape chip carrying the name, the
 * P/T, and the marks, with the face one gesture away (§3, step 6) — and *that* threshold is
 * `fit.ts`'s, derived from §2's 9px type floor, because the question is about type and this file
 * has never read a character. And the *printed* proportion, 63:88, holds at every size, so a short
 * row produces a narrow card no matter how much width is going spare: height is the thing that
 * runs out on a table, and a card that ignored it would be a card lying half in the next row.
 *
 * **Pure, and deliberately so.** Numbers in, numbers out: no React, no DOM, no `window`, no
 * measurement. Every case in this file — the clamp, the overlap switchover, the chip threshold,
 * one permanent and forty — is testable without a browser, which is what makes the whole
 * supported range checkable rather than the one screen somebody happened to open.
 *
 * **It decides no rule and reads no card.** Which row a permanent belongs to is `board.ts`'s
 * answer from the server's `card_types`, the order inside a row is the server's, and nothing here
 * has ever seen a name, a cost, or a type line. Packing decides sizes and positions and nothing
 * else — no packing decision may depend on a card's text (§5's opening line), which is the defect
 * this whole document exists to remove.
 */
import { CARD_MIN_HEIGHT, PRINTED_RATIO, presentationFor, type Presentation } from './fit'

/** A box to pack into, in CSS px. Where it came from is the caller's business. */
export interface Box {
  width: number
  height: number
}

/** Where one tile sits, relative to the top-left of the row it is in. */
export interface Point {
  x: number
  y: number
}

/** One row of permanents, packed: the tile it draws, and where each of them goes. */
export interface PackedRow {
  width: number
  height: number
  /**
   * The room one tile reserves along the row, which is the width it takes **turned**.
   *
   * A tapped permanent is a quarter turn (§6), so the space a permanent occupies is its own
   * height rather than its width — and it is charged whether or not anything is currently tapped,
   * because a reservation that appeared when a creature attacked would move every other card on
   * the board, which §5 forbids. The tile is drawn `width × height` inside it, centred.
   */
  footprint: number
  /**
   * The distance from one tile's left edge to the next one's.
   *
   * Greater than the footprint while there is room to space them and less than it once there is
   * not, which is the whole of §3's step 4: the row stops being a sequence and starts being a fan.
   */
  pitch: number
  /**
   * Whether the tiles cover one another. `pitch < footprint`, stated so a caller need not derive
   * it — and measured against the turned footprint rather than the upright width, because a row
   * whose tiles have no room to turn is one where tapping covers a neighbour.
   */
  overlapped: boolean
  /** How many lines of tiles the row uses. More than one only where the height is going spare. */
  lines: number
  /**
   * Whether this tile is drawn under §5's 72×100 permanent minimum — **reported, never hidden**.
   *
   * §3 makes that minimum a *review threshold* rather than a stop-drawing line: "when scaling
   * reaches a size that seems too small, the client still draws it, and the fact is reported to
   * the maintainer — who is the one who decides whether it has gone too far." So a field with the
   * room for three rows of 66px cards draws them, and says that it did. A chip is never under the
   * floor: it is a different tier with a floor of its own, not a small card.
   */
  belowFloor: boolean
  /**
   * The presentation these tiles will get, which is `fit.ts`'s answer for this box and not a
   * second opinion: a surface never names its own tier (§6). Reported because it is what the row
   * *is*, and a caller that wants to say so in a class name should not have to re-derive it.
   */
  tier: Presentation
  /** One point per tile, in the server's order. Never leaves the box. */
  positions: readonly Point[]
}

/** One row's place inside a field, and the packing of the permanents in it. */
export interface FieldRow {
  x: number
  y: number
  width: number
  height: number
  pack: PackedRow
}

/** A whole battlefield, divided into rows and packed. */
export interface FieldPlan {
  rows: readonly FieldRow[]
}

/** The two answers the scene's ladder hands a field (`scene.ts`, `SceneLadder`). */
export interface FieldOptions {
  rows: 'split' | 'merged'
  cardTier: 'designed' | 'compact' | 'chip'
}

/** What a field needs in order to divide itself the way the other half of the table did. */
export interface PackOptions {
  /** How many rows the *table* draws (`fieldSlots`). Never how many this half has to put in them. */
  slots: number
  /** Whether this half is drawn mirrored, which is what puts its creatures at the dividing line. */
  mirrored?: boolean
  cardTier: FieldOptions['cardTier']
}

/**
 * §5's permanent tile: 72×100 at the minimum, 130×182 designed.
 *
 * The minimum is measured off XMage, which fits a complete name, cost, type line, keyword line and
 * P/T into that box at roughly 9px — complete-and-small is readable where large-and-truncated is
 * not. **It is a review threshold, not a stop-drawing line** (§3, §5): a row too short for it
 * draws a smaller card and says so (`PackedRow.belowFloor`), and the only place it is still a hard
 * bound is *sideways*, where a row too crowded for it overlaps at full size rather than shrinking.
 */
const CARD_FLOOR: Box = { width: 72, height: 100 }
const CARD_DESIGNED: Box = { width: 130, height: 182 }

/**
 * §5's chip: what is drawn where a row can no longer set a name at all.
 *
 * Landscape precisely because a row that short cannot hold a portrait card, and it never shrinks —
 * it overlaps instead. Where the threshold is, is `fit.ts`'s answer and not a number here.
 */
const CHIP: Box = { width: 96, height: 30 }

/**
 * The gap between tiles: the minimum §5's formula divides by, and the size it is drawn at.
 *
 * Two numbers because §3 spends them in that order — gaps compress toward their minimums (step 1)
 * *before* cards shrink toward the floor (step 2). So the width is solved with the minimum, and
 * whatever is left over is given back to the gap up to a share of the tile, which is what keeps a
 * row of small cards from looking like a row of large ones with the air taken out.
 */
const GAP_MIN = 4
const GAP_SHARE = 0.07
const GAP_MAX = 10

/** Between the lines of a row that wrapped, and between the rows of a field. */
const LINE_GAP = 4
const ROW_GAP = 6

/** The field's own edge. Horizontal only: vertical inset is height the scene already budgeted. */
const INSET = 4

/**
 * Pack one row: §5's clamp, then §3's fan.
 *
 * The tile is sized from the height first and the width second, because a row's height is what
 * runs out on a table. `chip` is the caller's, from the ladder; a row too short to set a name at
 * §2's floor is a chip row whatever it was told, since that is a rule that can be evaluated
 * rather than a judgment call per screen — and `fit.ts` owns it, because it is a fact about type.
 *
 * **A row takes a second line before it fans.** §5 bounds the pitch below by "the width of a
 * legible name strip" and leaves the number open; what is actually available to honour that bound
 * is the row's own unused height, and using it is strictly better than a fan — every tile stays
 * whole, at a size the height still affords, rather than every tile after the first losing its
 * right-hand side.
 *
 * **The line count is chosen the same way the row count is: by the card it draws.** Every count
 * a line's own floor allows is scored, and the biggest tile wins; at equal size the row that
 * overlaps least wins, and at equal size and equal overlap the fewest lines, because a row read in
 * one movement is worth something and nothing else here is. Reading it as "the first count that
 * avoids overlap" instead — which is what it used to say — is the same mistake §3's "More screen
 * is never a worse board" names one level up: a 356px row wrapped five permanents onto two lines
 * of 73px cards, and a 360px row, with *more* room, drew one line of 72px ones. The objective is
 * the tile, at both levels, so widening a box can never shrink what is in it.
 *
 * Where no count avoids the fan the deepest one is still taken, because the alternative to a
 * fanned card is not a bigger card, it is a permanent nobody drew — and an object with no box can
 * be identified by no gesture at all.
 */
export function packRow(box: Box, count: number, opts: { chip?: boolean } = {}): PackedRow {
  const room = Math.max(0, Math.floor(box.width))
  const tall = Math.max(0, Math.floor(box.height))
  const many = Math.max(0, Math.floor(count))
  const chip = opts.chip === true || tall < CARD_MIN_HEIGHT
  const least = chip ? CHIP.height : CARD_MIN_HEIGHT

  let lines = 1
  let solved = solve(room, lineTall(tall, 1, chip), many, chip)
  // Every line count the tier's own minimum still leaves room for, and there is nothing to move
  // onto a line past the last permanent.
  for (let take = 2; take <= many && lineTall(tall, take, chip) >= least; take++) {
    const candidate = solve(room, lineTall(tall, take, chip), Math.ceil(many / take), chip)
    if (better(candidate, solved)) {
      lines = take
      solved = candidate
    }
  }

  const perLine = many === 0 ? 0 : Math.ceil(many / lines)
  // The gap between lines is the first thing to give when it is what stands between the row and
  // another line (§3, step 1), so the tiles were sized without it and it is spent afterwards.
  const gap = lines * solved.height + (lines - 1) * LINE_GAP <= tall ? LINE_GAP : 0
  // Centred in whatever the row did not need, so a row of chips in a taller box sits on the board
  // rather than clinging to the top of it.
  const block = lines * solved.height + (lines - 1) * gap
  const top = Math.max(0, Math.floor((tall - block) / 2))

  // The tile inside its own reserved footprint: half the turn's extra width on each side, so a
  // permanent that turns grows symmetrically into room that was already its.
  const inset = Math.round((solved.footprint - solved.width) / 2)

  const positions: Point[] = []
  for (let index = 0; index < many; index++) {
    const line = Math.floor(index / perLine)
    const column = index % perLine
    positions.push({
      // Clamped as well as computed: a fractional pitch rounded up on the last tile of a line is
      // a pixel of overflow, and overflow is the one thing this file is not allowed to produce.
      x: inset + Math.min(Math.max(0, room - solved.footprint), Math.round(column * solved.pitch)),
      y: top + line * (solved.height + gap),
    })
  }

  const tier = presentationFor({ width: solved.width, height: solved.height })
  return {
    width: solved.width,
    height: solved.height,
    footprint: solved.footprint,
    pitch: solved.pitch,
    overlapped: solved.overlapped,
    lines,
    tier,
    belowFloor: many > 0 && tier !== 'chip' && solved.height < CARD_FLOOR.height,
    positions,
  }
}

/** The height one of `lines` lines gets, before the gaps between them are paid for. */
const lineTall = (tall: number, lines: number, chip: boolean): number =>
  Math.min(chip ? CHIP.height : CARD_DESIGNED.height, Math.floor(tall / Math.max(1, lines)))

/** One line count's answer, before it is compared with the others. */
interface Solved {
  width: number
  height: number
  footprint: number
  pitch: number
  overlapped: boolean
}

/**
 * Whether one line count draws a better row than another: **the tile first**, then how little it
 * covers itself, and nothing else.
 *
 * The last clause is what keeps a comfortable row from wrapping: at equal size and with neither
 * row overlapping there is no reason to spend a second line, so the earlier — shallower — count
 * stands. Four chips in a 500px row stay in one line even though two lines of two would space
 * them further apart.
 */
const better = (candidate: Solved, best: Solved): boolean =>
  candidate.width !== best.width
    ? candidate.width > best.width
    : best.overlapped && (!candidate.overlapped || candidate.pitch > best.pitch)

/**
 * §5's clamp, for one line of `count` tiles in a box `room` wide and `tall` high.
 *
 * ```
 * ideal   = the band's designed footprint
 * fitted  = (W - (N-1)·g) / N
 * footprint = clamp(FLOOR, fitted, ideal)
 * ```
 *
 * **What is divided up is the turned footprint, not the upright width.** A tapped permanent is a
 * quarter turn (§6), so what a permanent occupies along a row is its own *height* — and the row
 * charges it whether or not anything is tapped, since a footprint that appeared when a creature
 * attacked would slide every other card on the board (§5). Height is therefore what a turn costs:
 * where the row is crowded enough for the clamp to bite, the tile it can afford is a shorter card
 * than the same room would have drawn upright, and the printed proportion takes the width from
 * there.
 *
 * The ideal is the height the line got, never a number of its own: a short line produces a narrow
 * card however much width is going spare, because the alternative is a card lying half in the line
 * below it. **And it is not lifted back to 72×100**: where the line is shorter than that, the card
 * is smaller than that, which is the whole of §5's soft floor downward.
 */
function solve(room: number, tall: number, count: number, chip: boolean): Solved {
  // A chip is landscape and fixed: it is already the floor, so there is nothing under it to
  // shrink to and a crowded chip row overlaps from the start. It never turns either — a 96×30
  // chip is already lying down — so its footprint is simply its width.
  const ideal = chip ? CHIP.width : Math.min(CARD_DESIGNED.height, tall)
  // **Sideways the floor is hard.** A row too crowded for the minimum tile overlaps at full size
  // rather than shrinking past it — but a row too *short* for the minimum tile has already drawn a
  // smaller one, and inflating its footprint back to 100 would reserve room for a card that is not
  // there. So the bound the width divides against is the minimum, or the line's own tile where
  // that is smaller.
  const floor = chip ? CHIP.width : Math.min(CARD_FLOOR.height, ideal)

  // The gap here is the minimum §5 divides by; the drawn one is decided below, out of whatever
  // the clamp left behind.
  const fitted = count === 0 ? ideal : Math.floor((room - (count - 1) * GAP_MIN) / count)
  // A box narrower than one tile is a box sized wrong (§6). The tile is drawn at what there is
  // rather than hanging over the edge, because the fix belongs to whatever chose the box.
  const footprint = Math.min(room, Math.max(floor, Math.min(fitted, ideal)))
  const height = chip ? tall : Math.min(tall, footprint)
  // Rounded rather than floored, so the proportion is the printed one at every size instead of
  // needing 72 asserted back over it at the minimum: 100px of card is 72px wide by rounding, and
  // 66px of card is 47px wide by the same arithmetic.
  const width = chip ? footprint : Math.min(footprint, Math.round(height * PRINTED_RATIO))

  const gap = Math.min(GAP_MAX, Math.max(GAP_MIN, Math.round(width * GAP_SHARE)))
  const spread = count > 1 ? (room - footprint) / (count - 1) : 0
  // Spacing while it fits, overlap once it does not — and in between, a gap that compresses
  // rather than a card that shrinks. All three come out of one `min`, so there is no threshold
  // for a rounding error to fall either side of.
  const pitch = count > 1 ? Math.min(footprint + gap, spread) : 0
  return { width, height, footprint, pitch, overlapped: count > 1 && pitch < footprint }
}

/**
 * How many rows a table draws: **one per group the board has**, and that is the whole rule.
 *
 * §5, "The row count is the board's, never the card's": a field draws one row per group the
 * server's `card_types` produced — creatures, other permanents, lands — and **that count does not
 * fall to buy card size**. Where three rows will not fit at 100px each, the answer is three rows
 * of smaller cards (§3, "The split is kept, and the cards get smaller"). The scan by category is
 * how a board is read at a glance and it is worth far more than any particular card size.
 *
 * This used to pack every row count from one upward and take the one whose worst row drew the
 * biggest tile. **That objective always merges.** One row of `N` cards is never smaller than three
 * rows of the same `N` — the merged row can reproduce any split, same line height with the count
 * spread evenly instead of by category, and usually beats it — so the "at equal size the deeper
 * split wins" tie-break almost never fired and every desktop below ultrawide drew its creatures,
 * artifacts and lands in one row. It was obeying the spec of the time, which made the 100px floor
 * hard and turned any shorter row into chips; §5 now makes that floor soft downward, so keeping
 * the split costs card size instead of costing card faces.
 *
 * **The count falls only where a row cannot draw a tile at all** — not where it would draw a small
 * one. The bound is the tier's own minimum: the chip's 30px, which is the smallest tile any tier
 * draws, since a row too short for a card already draws a chip instead (`packRow`). A row with
 * less height than that draws nothing, and nothing is worse than a merged row. That is §3's step
 * 6 and the bottom of the ladder.
 *
 * The ladder's answer still comes first: where the scene already merged, this cannot un-merge.
 *
 * **Answered once for the whole table, out of every half's groups.** Both halves then divide their
 * field the same way, and a permanent is the same size at both ends of it — a board where one
 * seat's creatures are two thirds the size of the other's answers "whose is this" with something
 * other than where it is. The price is that a half with fewer groups than the table leaves its far
 * slot empty, and it is the right price: the empty slot is at the edge away from the dividing line,
 * where it reads as the absence it is. A half with *more* groups than the table gives up the
 * split, from the edge away from the dividing line inward (`Battlefield.tsx`).
 *
 * **It reads no card, and now not even a count.** `groups` is how many groups each half drew, a
 * single number per half — `board.ts` decided the grouping from what the server stated, and how
 * many permanents are in one is the cards' problem rather than the row count's. The *structure* of
 * a field answers to what is on the board; its *box* never does (§5).
 */
export function fieldSlots(box: Box, groups: readonly number[], ladder: FieldOptions): number {
  if (ladder.rows === 'merged') return 1
  const height = Math.max(0, Math.floor(box.height))
  const floor = ladder.cardTier === 'chip' ? CHIP.height : CARD_FLOOR.height
  let rows = Math.max(1, ...groups.map((count) => Math.max(0, Math.floor(count))))
  // Down one row at a time, so the answer is the deepest split the field can still draw anything
  // in rather than a jump to one row the moment the deepest does not fit.
  while (rows > 1 && slotHeight(height, rows, floor) < CHIP.height) rows--
  return rows
}

/**
 * Divide a field into the table's rows and pack each of them.
 *
 * `counts` is one number per row *this half* draws, in `board.ts`'s order — already merged into a
 * single entry by the caller where `fieldSlots` said the table draws one row. An empty group is
 * not among them and costs no row at all, which is the other half of what "the place stays, the
 * sentence goes" means.
 *
 * The rows this half has are laid into the slots nearest the middle of the table, so a mirrored
 * half keeps its creatures against the dividing line whether or not it has lands to put behind
 * them. Every slot is the same height, and the gap between them is what gives first: the scene
 * budgets two rows of exactly the card minimum, so a gap taken off the top would put both rows a
 * few pixels under it for three pixels of air. §3's step 1 compresses gaps before anything else
 * gives way, and `slotHeight` is where that happens.
 */
export function packField(box: Box, counts: readonly number[], opts: PackOptions): FieldPlan {
  const width = Math.max(0, Math.floor(box.width) - 2 * INSET)
  const height = Math.max(0, Math.floor(box.height))
  const slots = Math.max(1, Math.floor(opts.slots), counts.length)
  if (counts.length === 0 || width <= 0 || height <= 0) return { rows: [] }

  const chip = opts.cardTier === 'chip'
  const floor = chip ? CHIP.height : CARD_FLOOR.height
  const each = slotHeight(height, slots, floor)
  const gap = rowGap(height, slots, each)
  const offset = opts.mirrored === true ? slots - counts.length : 0

  return {
    rows: counts.map((count, index) => ({
      x: INSET,
      y: (offset + index) * (each + gap),
      width,
      height: each,
      pack: packRow({ width, height: each }, count, { chip }),
    })),
  }
}

/** One row's share of a field's height, once the gaps between the rows are paid for. */
const rowHeight = (height: number, rows: number, gap: number): number =>
  rows <= 0 ? 0 : Math.max(0, Math.floor((height - (rows - 1) * gap) / rows))

/**
 * The height one of `rows` rows gets: its share after the gaps, or `floor` where giving the gaps
 * up is what holds the tile at it.
 *
 * §3's step 1 — gaps compress toward their minimums before anything else gives way — and the
 * clause is bounded rather than a switch, which is what makes it **monotone**. Both terms are
 * non-decreasing in the height, so their maximum is, and a field one pixel taller can never draw a
 * shorter row. Written as a switchover instead — the gap appearing whole the moment the rows clear
 * the floor with it — a 312px field drew three 100px rows where a 311px one drew three 103px rows,
 * which is exactly §3's "More screen is never a worse board" broken over six pixels of air.
 */
const slotHeight = (height: number, rows: number, floor: number): number =>
  Math.max(rowHeight(height, rows, ROW_GAP), Math.min(floor, rowHeight(height, rows, 0)))

/** The gap actually drawn between the rows: what they left over, up to `ROW_GAP`. */
const rowGap = (height: number, rows: number, each: number): number =>
  rows <= 1 ? 0 : Math.min(ROW_GAP, Math.max(0, Math.floor((height - rows * each) / (rows - 1))))
