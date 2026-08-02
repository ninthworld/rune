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
 * When `fitted` falls under the floor the row stops spacing and starts **overlapping**: the pitch
 * becomes `(W - footprint) / (N - 1)` and the cards keep their size. That direction is the whole
 * point — a card below the floor is unreadable everywhere at once, while an overlapped row is
 * unreadable only where it is covered, and what stays exposed is the top-left strip, which is the
 * name band. A fanned row is read as its names.
 *
 * One thing comes before the fan, and it is the only addition to §5 here: **a row takes a second
 * line while it has the height for one at the same tile size.** A chip is 30px in a 60px box and a
 * card that has shrunk to its floor is 101px in a field that was 229 — in both, the height going
 * spare buys a line of whole tiles where the fan would have cost every tile after the first its
 * right-hand side. It is never bought with the tile: a line paid for by shorter cards would be
 * §3's step 6 taken early, and the card is what the ladder protects longest.
 *
 * Two things the row height decides, and they are §4's rule rather than a judgment per screen.
 * **A permanent is a card while its row is at least 100px tall**; below that it is a landscape
 * chip carrying the name, the P/T, and the marks, with the face one gesture away (§3, step 6) —
 * rendering a 60px "card" instead is a shape with the outline of information and none of the
 * substance. And the *printed* proportion, 63:88, holds at every size, so a short row produces a
 * narrow card no matter how much width is going spare: height is the thing that runs out on a
 * table, and a card that ignored it would be a card lying half in the next row.
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
import { presentationFor, type Presentation } from './fit'

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

/** The printed proportion, as a multiplier from height to width. Held at every size (§5). */
const RATIO = 63 / 88

/**
 * §5's permanent tile: 72×100 at the floor, 130×182 designed.
 *
 * The floor is measured off XMage, which fits a complete name, cost, type line, keyword line and
 * P/T into that box at roughly 9px — complete-and-small is readable where large-and-truncated is
 * not. It is a floor for the *card*, not for the tile: below it a permanent stops being a card
 * rather than becoming a smaller one.
 */
const CARD_FLOOR: Box = { width: 72, height: 100 }
const CARD_DESIGNED: Box = { width: 130, height: 182 }

/** §5's chip, drawn below a 100px row. Landscape, and it never shrinks — it overlaps instead. */
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
 * runs out on a table. `chip` is the caller's, from the ladder; a row shorter than the card floor
 * is a chip row whatever it was told, since §4 makes that a rule that can be evaluated rather
 * than a judgment call per screen.
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
  const chip = opts.chip === true || tall < CARD_FLOOR.height
  const floor = chip ? CHIP : CARD_FLOOR

  let lines = 1
  let solved = solve(room, lineTall(tall, 1, chip), many, chip)
  // Every line count the tier's floor still leaves room for, and there is nothing to move onto a
  // line past the last permanent.
  for (let take = 2; take <= many && lineTall(tall, take, chip) >= floor.height; take++) {
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

  return {
    width: solved.width,
    height: solved.height,
    footprint: solved.footprint,
    pitch: solved.pitch,
    overlapped: solved.overlapped,
    lines,
    tier: presentationFor({ width: solved.width, height: solved.height }),
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
 * below it.
 */
function solve(room: number, tall: number, count: number, chip: boolean): Solved {
  // A chip is landscape and fixed: it is already the floor, so there is nothing under it to
  // shrink to and a crowded chip row overlaps from the start. It never turns either — a 96×30
  // chip is already lying down — so its footprint is simply its width.
  const ideal = chip
    ? CHIP.width
    : Math.max(CARD_FLOOR.height, Math.min(CARD_DESIGNED.height, tall))
  const floor = chip ? CHIP.width : CARD_FLOOR.height

  // The gap here is the minimum §5 divides by; the drawn one is decided below, out of whatever
  // the clamp left behind.
  const fitted = count === 0 ? ideal : Math.floor((room - (count - 1) * GAP_MIN) / count)
  // A box narrower than one tile is a box sized wrong (§6). The tile is drawn at what there is
  // rather than hanging over the edge, because the fix belongs to whatever chose the box.
  const footprint = Math.min(room, Math.max(floor, Math.min(fitted, ideal)))
  const height = chip ? tall : Math.min(tall, footprint)
  const width = chip
    ? footprint
    : Math.min(footprint, Math.max(CARD_FLOOR.width, Math.floor(height * RATIO)))

  const gap = Math.min(GAP_MAX, Math.max(GAP_MIN, Math.round(width * GAP_SHARE)))
  const spread = count > 1 ? (room - footprint) / (count - 1) : 0
  // Spacing while it fits, overlap once it does not — and in between, a gap that compresses
  // rather than a card that shrinks. All three come out of one `min`, so there is no threshold
  // for a rounding error to fall either side of.
  const pitch = count > 1 ? Math.min(footprint + gap, spread) : 0
  return { width, height, footprint, pitch, overlapped: count > 1 && pitch < footprint }
}

/**
 * How many rows a table draws — §3's step 5, applied to the room a field actually got rather
 * than to the band it is in.
 *
 * **The row count is chosen to maximise card size, not to maximise rows** (§3, "More screen is
 * never a worse board"). Rows are not inherently better: splitting into creatures, other
 * permanents and lands buys a *scan by category* and costs card size, because the same height
 * divided three ways draws smaller cards. Asking only whether each row clears the 100px floor is
 * the ladder read as a checklist — a field with just enough height to squeeze three rows past it
 * draws three rows of clipped 75px cards where one merged row would draw complete 130px ones, and
 * a bigger screen that draws a smaller card is the one thing §3 says is never allowed.
 *
 * So every row count from one up to the one the table needs is **packed**, and the one whose
 * *worst* row draws the biggest tile wins. Packed rather than estimated from the row height,
 * because the height is only half of it: a merged row can spend its own spare height on a second
 * line of whole tiles and a split row is already too short to, so the choice between them turns on
 * the count as well as the room. Scored on the worst row and not the average, since a board is
 * only as readable as the smallest card on it.
 *
 * Rows count for exactly one thing, and it is the tie-break: **at equal card size the deeper split
 * wins**, because there the scan by category is free. It is never more than a tie-break, and that
 * is what makes the answer monotone: a merged row can reproduce any split — same line height, and
 * the count spread evenly instead of by category — so merging is never the smaller card, the
 * winning size is always the merged one, and the merged one is non-decreasing in the box. The
 * split is kept precisely while it costs nothing.
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
 * **It still reads no card.** `halves` is how many permanents are in each of a half's groups, which
 * is a count and a shape, never a name or a type — `board.ts` decided the grouping and the server
 * decided that. What it means is that the *structure* of a field answers to what is on the board
 * while the *box* never does, which is the line §5 draws: the region is the viewport's, the tiles
 * inside it are the count's.
 */
export function fieldSlots(
  box: Box,
  halves: readonly (readonly number[])[],
  ladder: FieldOptions,
): number {
  if (ladder.rows === 'merged') return 1
  const most = Math.max(1, ...halves.map((half) => half.length))
  if (most === 1) return 1
  const room = Math.max(0, Math.floor(box.width) - 2 * INSET)
  const height = Math.max(0, Math.floor(box.height))
  const chip = ladder.cardTier === 'chip'

  let best = 1
  let winner = worstRow(room, height, halves, 1, chip)
  for (let rows = 2; rows <= most; rows++) {
    const tile = worstRow(room, height, halves, rows, chip)
    // Not *worse*, walking upward: at equal card the deeper split wins.
    if (!poorer(tile, winner)) {
      winner = tile
      best = rows
    }
  }
  return best
}

/** The smallest tile on the table, if it divided both its halves into `rows` rows. */
function worstRow(
  room: number,
  height: number,
  halves: readonly (readonly number[])[],
  rows: number,
  chip: boolean,
): PackedRow {
  const box: Box = {
    width: room,
    height: slotHeight(height, rows, chip ? CHIP.height : CARD_FLOOR.height),
  }
  let worst: PackedRow | undefined
  for (const half of halves) {
    for (const count of collapseCounts(half, rows)) {
      const packed = packRow(box, count, { chip })
      if (worst === undefined || poorer(packed, worst)) worst = packed
    }
  }
  return worst ?? packRow(box, 0, { chip })
}

/**
 * Which of two tiles is the poorer board.
 *
 * The tier leads and the size follows, because **a chip is not a narrow card**: giving up the face
 * is §3's step 6 and it loses to any card, including one narrower than the 96px chip. Comparing
 * widths alone would call a row of chips the better answer for being wider.
 *
 * Height breaks a tie in width, which only ever matters to chips — a card's height is its width
 * through the printed proportion, so the two cannot disagree there. A chip has no narrower size to
 * shrink to, but a row too short to hold one squashes it, and 96×25 is the poorer tile.
 */
const poorer = (tile: PackedRow, than: PackedRow): boolean =>
  (tile.tier === 'chip') !== (than.tier === 'chip')
    ? tile.tier === 'chip'
    : tile.width !== than.width
      ? tile.width < than.width
      : tile.height < than.height

/**
 * The counts `rows` rows draw, for a half that has more groups than that.
 *
 * The arithmetic of `Battlefield.tsx`'s collapse and nothing else — which groups merge is that
 * file's decision, and it does not change the multiset this returns, so it does not change which
 * row count wins.
 */
const collapseCounts = (counts: readonly number[], rows: number): readonly number[] =>
  counts.length <= rows
    ? counts
    : [...counts.slice(0, rows - 1), counts.slice(rows - 1).reduce((all, one) => all + one, 0)]

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
 * them. Every slot is the same height, and they are equal *before* the gap between them is paid
 * for: the scene budgets two rows of exactly the card floor, so a gap taken off the top would put
 * both rows a few pixels under it and turn a board of cards into a board of chips over three
 * pixels of air. The gap is drawn only where the rows can still hold their tile without it —
 * §3's step 1, which compresses gaps before anything else gives way.
 */
export function packField(box: Box, counts: readonly number[], opts: PackOptions): FieldPlan {
  const width = Math.max(0, Math.floor(box.width) - 2 * INSET)
  const height = Math.max(0, Math.floor(box.height))
  const slots = Math.max(1, Math.floor(opts.slots), counts.length)
  if (counts.length === 0 || width <= 0 || height <= 0) return { rows: [] }

  const chip = opts.cardTier === 'chip'
  const floor = chip ? CHIP.height : CARD_FLOOR.height
  const gap = rowGap(height, slots, floor)
  const each = rowHeight(height, slots, gap)
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

/** The gap between rows, drawn only where the rows still hold their tile without it (§3, step 1). */
const rowGap = (height: number, rows: number, floor: number): number =>
  rowHeight(height, rows, ROW_GAP) >= floor ? ROW_GAP : 0

/** The height one of `rows` rows gets, gap included where the field can afford to draw one. */
const slotHeight = (height: number, rows: number, floor: number): number =>
  rowHeight(height, rows, rowGap(height, rows, floor))
