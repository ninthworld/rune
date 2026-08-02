/**
 * The scene: a viewport goes in, and every region's box comes out.
 *
 * The client's geometry used to be the residue of its content — a row grew because permanents
 * arrived in it, a battlefield that ran out of room grew a scrollbar, and half the table moved
 * when the other seat's last creature died. `docs/client-design.md` §5 says geometry is
 * *computed* instead, and this is where the computing happens. One pure function answers both
 * questions a layout asks: which arrangement this shape of screen is in, and how much room each
 * region gets in it.
 *
 * **Regions are sized by the viewport; counts are absorbed by the cards.** How many permanents a
 * seat controls and how many cards are in a hand change no box here, because a count belongs to
 * the things inside a region rather than to the region: fifteen permanents in a field sized for
 * six make the cards smaller, then overlapped, then chips, by §5's `clamp(FLOOR, fitted, ideal)`.
 * So both battlefields are always the same height and the line across the middle of the table
 * does not move for any game event — the layout a player learns on their first turn is the layout
 * they have on their twentieth. An empty half of the table is how "my opponent has nothing" is
 * read; a missing region is not. (The sentence *inside* an empty field is a different defect and
 * a real one — a card row's height spent printing "No permanents." — and it belongs to the
 * surfaces that draw the field, not to the arithmetic that sizes it.)
 *
 * **Nothing here can express overflow.** There is no field for it, no flag that turns it on, and
 * no rect that is allowed to leave the viewport. When the regions do not fit, the ladder tightens
 * until they do — cards toward their floor, rows merged, faces down to chips, rails collapsed —
 * and if the viewport is below the supported floor the band says `unsupported` and the caller
 * draws a notice instead of a broken board (§3, "No region of the board ever scrolls").
 *
 * **It knows nothing about the game.** This module never sees a `GameView`, never reads
 * `card_types`, and never decides anything a rule decides. The only number it takes about the
 * table is the stack's depth, and it reads nothing from that but whether it is zero. `asking` is
 * the other input and it is a *presentation* fact: the dock's own tone (`dock.ts`), which is what
 * decides whether the hand or the controls own the bottom band.
 *
 * **It knows nothing about the browser, either.** No React, no DOM, no `window`, no `matchMedia`.
 * That is what makes every band in §4 testable without one — the bands are arithmetic on two
 * numbers, and a phone, a tablet, and a desktop at 200% zoom are the same arithmetic.
 *
 * **No region ever shrinks as the viewport grows.** §3's "More screen is never a worse board" is
 * a property, and the only way to have it is to build for it: every size below is a *ramp* rather
 * than a step. A chrome element that wants more room at a threshold — the match line at its
 * designed height, the hand back at its floor, a rail where there was a badge — takes it out of
 * the room the threshold *brought with it*, one pixel per pixel, and never out of a region that
 * was already sized. Where the surplus has not covered a rail yet, the width is held aside rather
 * than lent to the board and taken back a pixel later: a hundred pixels of gutter for a hundred
 * pixels of width is the price of a board that never gets smaller, and it is cheap.
 *
 * That is also why **the arrangement across the table is a function of width alone, and down it
 * of height alone**. A rail that appeared because the *ratio* crossed 0.8 would vanish again when
 * the same screen got taller, and the board would lurch by two hundred pixels in both directions;
 * §4's own test for a per-band difference is "name the thing that does not fit", and what does not
 * fit beside a rail is width. The band still names the shape of the screen — it is what tells a
 * surface whether a seat is a column or a bar — but it no longer decides a box.
 *
 * Two places where the spec's own sections point in different directions, and what was done:
 *
 * - §4 gives the Wide band as "ratio ≥ 1.3, height ≥ 640", which leaves 853×480 — a 1280×720
 *   desktop at 150% zoom — in no band at all. The height in that row describes where the
 *   *designed* sizes survive, and that is now something the allocation below answers rather than
 *   something a band asserts, so the band is chosen by ratio alone once the Short floor is past.
 * - §4's Tall row says rows merge and §3 puts merging *after* chips. Merging is what keeps a
 *   permanent a card on a phone — one row of 163px beats three of 51px — so it is applied first,
 *   and by the room a field actually got rather than by the band. A 844px-tall phone keeps its
 *   rows split; a 667px one merges them; both keep card faces, which is what §4 promises.
 */

import { CARD_MIN_HEIGHT } from './fit'
import { ROW_GAP } from './pack'

/** Which arrangement §4 puts this shape of screen in. */
export type Band = 'ultrawide' | 'wide' | 'square' | 'tall' | 'short' | 'unsupported'

/** The layout viewport, in CSS px. At 200% zoom a 1280×720 screen reports 640×360. */
export interface Viewport {
  width: number
  height: number
}

/**
 * The little the arrangement needs to know about the table.
 *
 * Not much, deliberately. A region's height and position are a function of the viewport alone,
 * so there is nowhere here to put a permanent count or a hand size — they would have nothing to
 * change, and a dead input is an invitation to wire it back into the geometry. What is left is
 * §5's two bounded departures, and both of them answer *what is happening* rather than *how much
 * stuff there is*. That distinction is the whole test.
 */
export interface SceneCounts {
  /**
   * Objects on the stack. Only whether it is zero is ever read.
   *
   * The stack is an event and a battlefield is a place: an event that is not happening takes no
   * room, so an empty stack has no box and the board takes the width back, while a place at the
   * table does not stop existing because nobody has put anything on it. Note what this does not
   * license — the box is decided by whether the stack exists, never by how deep it is. A
   * seven-deep stack and a one-deep stack get the same rail, and the depth is absorbed by the
   * items in it exactly as a permanent count is.
   */
  stackDepth: number
  /**
   * Whether the game is mid-question — `dock.ts`'s `asking` or `confirm` tone.
   *
   * The hand and the action affordance both want the bottom band on a small screen and they take
   * turns for it (§2): the dock takes what it needs to carry a question before the hand grows past
   * its floor, so a screen with room for both keeps both and a screen without one gives the room to
   * the question that is waiting. Where that leaves the hand under a hand card's height it is the
   * peek strip, and restoring it is a gesture — a hand raised over a peek strip is an overlay
   * rather than a region, the same class of thing as an opened pile, because the space it would
   * need belongs to tier-1 minimums the board cannot give up.
   *
   * This is a change of *mode*, not a measure of content, which is what makes it a departure §5
   * allows where an empty battlefield is not one.
   */
  asking?: boolean
}

/** A box in scene coordinates: the origin is the viewport's top-left corner. */
export interface Rect {
  x: number
  y: number
  width: number
  height: number
}

/**
 * The regions a table is made of, in the reading order §4 fixes: opponent, board, you, hand.
 *
 * `side` is preview, log, and settle — the one region that is allowed to be absent, because §3's
 * last step turns it into a drawer, and a drawer is drawn over the board rather than beside it.
 */
export type RegionName =
  | 'header'
  | 'turn'
  | 'opponentSeat'
  | 'opponentField'
  | 'stack'
  | 'yourField'
  | 'yourSeat'
  | 'dock'
  | 'hand'
  | 'side'

/**
 * Which of §3's steps are in force, at the size this actually came out.
 *
 * Every one of these is read off the room that was left *after* the allocation below, never off
 * the band: two viewports in the same band with different amounts of height are two different
 * boards, and the point of a stated ladder is that it is derivable rather than hand-tuned per
 * screen.
 *
 * There is no `overflow` here and no `scroll`. That is the contract, not an omission.
 */
export interface SceneLadder {
  /** Step 7. `collapsed` means the turn is a row under the header rather than a rail at the edge. */
  rails: 'full' | 'collapsed'
  /** Step 6. `merged` means a battlefield draws one row, not `board.ts`'s three. */
  rows: 'split' | 'merged'
  /** Steps 2–5, as §6's presentations: what a permanent's box can still carry. */
  cardTier: 'designed' | 'compact' | 'chip'
  /** Step 8. A drawer's box is empty here — it is drawn over the board, not beside it. */
  sidePanel: 'column' | 'drawer'
}

/** One arrangement, fully determined. Everything a surface needs and nothing it can argue with. */
export interface Scene {
  band: Band
  /** Every region's box, absolutely positioned. Never derived from content. */
  regions: Readonly<Record<RegionName, Rect>>
  ladder: Readonly<SceneLadder>
}

/**
 * The floor and the size a region is drawn at when there is room — §5's table, in effective px.
 *
 * The numbers are the spec's, not this module's. Where the spec gives a card size, the region's
 * size is that plus the gutter around it, and nothing here adds an allowance the spec did not.
 */
interface Size {
  min: number
  designed: number
}

/** One line about the match. Chrome's type floor is 11px (§7), so the band is thin either way. */
const HEADER: Size = { min: 40, designed: 48 }
/** Where the match line can start paying for its designed height, out of the height past it. */
const HEADER_FROM = 640
/** The turn as a vertical rail: twelve steps, each of them the control that sets a stop there. */
const TURN_RAIL: Size = { min: 88, designed: 112 }
/**
 * The turn as a row under the header, where the width has no rail in it.
 *
 * One height, whether the row carries all twelve steps or the current one alone (§3, step 7).
 * They used to be 32 and 28, and four pixels a *wider* screen took off the fields is exactly the
 * defect this module is now built against — a row is a row, and which of the two it draws inside
 * that row is a fitting decision with no geometry in it.
 */
const TURN_ROW = 32
/** A stack item is 130 wide designed and a chip is 96 (§5); the rail is that plus its gutter. */
const STACK_RAIL: Size = { min: 104, designed: 150 }
/**
 * Collapsed: the top item's name and a count, which §3 says never degrades further.
 *
 * The rail's floor width, laid down instead of up, so that the rail which replaces it is never
 * narrower than the badge was — the same chip is drawn either way.
 */
const STACK_BADGE = STACK_RAIL.min
/** Name, life, status marks, and the fold that opens the five zone counts (§2, "The seat bar"). */
const SEAT: Size = { min: 40, designed: 56 }
/** The action affordance. Fixed place, always present — §2 admits no size at which it is not. */
const DOCK: Size = { min: 44, designed: 64 }
/** What the dock grows to when there is something to answer, funded by the hand it displaced. */
const DOCK_ASKING = 160
/** §5's hand card: 100×140 at the floor, 150×209 designed, plus the gutter under it. */
const HAND: Size = { min: 148, designed: 217 }
/** Enough of every card to count them, and no more. The hand is small, never invisible (§2). */
const HAND_PEEK = 48
/**
 * The height at which the hand starts buying its floor back, a pixel at a time.
 *
 * §4's Short band — height under 480 — makes the peek strip the *resting* state, and the hundred
 * pixels between the strip and a hand card have to come from somewhere. They come from the
 * hundred pixels of screen above the Short floor and from nothing else, so `640×476`'s card faces
 * survive `640×480` instead of being spent on a hand that suddenly wanted its whole floor.
 */
const HAND_FROM = 480
/** §5's chip is 96×30. A field that has gone to chips still draws one row of them. */
const CHIP_ROW = 36
/**
 * §5's permanent tile: 72×100 at the minimum, 130×182 designed.
 *
 * The minimum is a **review threshold** and not a switch (§3, §5): a row under it draws a smaller
 * card rather than a chip, and says that it had to. It is what a field *asks* for — the height it
 * claims before anything else buys its designed size — and never what it settles for.
 */
const CARD_ROW: Size = { min: 100, designed: 182 }
/**
 * The height below which a row cannot set a name, and so cannot draw a card at all.
 *
 * `fit.ts`'s, derived from §2's 9px type floor, and imported rather than restated: this is the one
 * threshold in the whole layout that decides whether a permanent is a card or a chip, and a copy
 * of it here is a second number to keep in step. It is what the *bottom* of the ladder is measured
 * against — where the rows merge, and where a field is a chip board — while the review threshold
 * above is what the budget aims at.
 */
const CARD_MIN = CARD_MIN_HEIGHT
/** Preview, log, and settle as a column. Below its floor it is a drawer instead (§3, step 8). */
const SIDE: Size = { min: 260, designed: 320 }
/** §4, Ultrawide: the board's content is held to a width a glance does not have to travel. */
const BOARD_MAX = 1440
/** The width the board keeps whatever else happens: a rail falls back to its floor before this does. */
const BOARD_MIN = 320
/**
 * How many rows a split battlefield is budgeted for: **every row the board can produce.**
 *
 * `board.ts` produces three — creatures, other permanents, lands — and §5 says a field draws one
 * row per group it produced, a count that does not fall to buy card size. A field budgeted for two
 * is therefore a field that is one row short of what it will be asked to draw, and the third row
 * comes out of the other two.
 *
 * It was 2, and the argument for it was that a portrait phone cannot keep card faces if its field
 * is cut three ways. That was true while the 100px row was a hard floor and anything under it was a
 * chip; #685 made the floor soft downward, so a field cut three ways draws *smaller cards* and the
 * argument no longer holds. What the old constant did instead was hand 1280×720 and 1440×900 a
 * field sized for two rows and three rows to put in it, which is 47×66 and 54×76 tiles and names
 * cut at the 9px floor — the same class of consequence as every other rule that outlived the floor
 * it was written against.
 *
 * **It is a constant, and it stays one.** A field budgeted for the rows the board *currently* has
 * would be a region sized by its contents, which is the one thing §5 forbids outright: both
 * battlefields are this height whether they hold three rows, one, or none.
 */
const SPLIT_ROWS = 3

/**
 * What a split field costs at a given per-row height — the rows, and the gaps `pack.ts` draws
 * between them.
 *
 * The gaps are charged here because they are charged there. A budget of `rows × height` hands back
 * a field that is exactly `(rows - 1) × ROW_GAP` short of holding what it was budgeted for, and a
 * threshold six pixels out of step is how a board ends up one row smaller than the arithmetic that
 * sized it believed.
 */
const fieldOf = (row: number): number => SPLIT_ROWS * row + (SPLIT_ROWS - 1) * ROW_GAP

/** Every rail at its designed width — what the full arrangement costs the width. */
const RAILS = TURN_RAIL.designed + STACK_RAIL.designed
/**
 * The narrowest viewport that could hold the whole arrangement beside a minimum board.
 *
 * Below it no rail is paid for at all, so a phone spends nothing on rails it will never draw.
 * From here the rails are bought a pixel per pixel of new width, and by `RAILS_FROM + RAILS` they
 * are all drawn and the board is growing again — never smaller than the `BOARD_MIN` it had when
 * the saving started.
 */
const RAILS_FROM = BOARD_MIN + RAILS

/**
 * The rails, in the order width buys them, each entry the whole arrangement at that price.
 *
 * The stack goes first because it is tier 1 whenever it is non-empty; the turn rail is next, at
 * its floor before its designed width, because a rail at 88 is still twelve steps and the board
 * is what the difference is worth; the stack's designed width is last, since a stack item at the
 * chip size is a name and a count and that is what §3 says never degrades.
 */
const RAIL_STEPS: readonly { turn: number; stack: number }[] = [
  { turn: 0, stack: 0 },
  { turn: 0, stack: STACK_RAIL.min },
  { turn: TURN_RAIL.min, stack: STACK_RAIL.min },
  { turn: TURN_RAIL.designed, stack: STACK_RAIL.min },
  { turn: TURN_RAIL.designed, stack: STACK_RAIL.designed },
]

const EMPTY: Rect = { x: 0, y: 0, width: 0, height: 0 }

/**
 * Whether the hand's box is the peek strip of §2 rather than a hand.
 *
 * Read off the box, not off a band or a mode: the hand is a strip exactly when §5's hand card
 * does not fit in the room it was given, which is the same question a surface would ask of any
 * other box. It is the scene's number, so it is answered here rather than by a component holding
 * a copy of 148.
 */
export const peeking = (hand: Rect): boolean => hand.height < HAND.min

/**
 * What a region may take at a threshold: the room past it, and never more than it wants.
 *
 * The whole of §3's "More screen is never a worse board" as arithmetic. A size that steps up at a
 * threshold pays for the step out of the surplus the threshold brought — a pixel per pixel — so
 * what is left for everything else is flat across the step instead of falling off it. Below the
 * threshold it is the floor, `target - floor` pixels above it the target, and in between it is
 * exactly as big as the extra screen allows.
 */
const ramp = (available: number, from: number, floor: number, target: number): number =>
  Math.max(floor, Math.min(target, floor + (available - from)))

/**
 * Which band a viewport is in.
 *
 * Ordered by what is scarce rather than by ratio alone: below the floor there is no arrangement
 * to state, and below 480px of height the scarce resource is height no matter how wide the
 * screen is, which is the whole of what the Short band means. Everything past that is the ratio,
 * and the thresholds are §4's.
 *
 * It names the *shape* of the screen and nothing else. No box below is a function of it: what a
 * rail costs is width, what a row costs is height, and a band is a ratio — so an arrangement
 * chosen by one would change under a screen that only got bigger.
 */
function bandOf(width: number, height: number): Band {
  // §1's floor, measured on the edges rather than on width and height, because a viewport does
  // not stop being too small by being turned sideways.
  if (Math.min(width, height) < 320 || Math.max(width, height) < 480) return 'unsupported'
  if (height < 480) return 'short'
  const ratio = width / height
  if (ratio < 0.8) return 'tall'
  if (ratio < 1.3) return 'square'
  if (ratio < 2) return 'wide'
  return 'ultrawide'
}

/**
 * The heights of the regions stacked between the chrome.
 *
 * One number for both seats and one for both battlefields, rather than four. The guarantee is
 * that the two halves of the table are mirror images whatever is on them, and a shape with a
 * `yourField` and an `opponentField` in it is a shape in which they could differ.
 */
interface Column {
  seat: number
  field: number
  dock: number
  hand: number
}

/**
 * Divide the height between the regions stacked down the middle of the table.
 *
 * The order is §2's contract read literally: **every tier-1 region takes its floor first** — both
 * seats, the dock, the hand, and both battlefields — and only then is the surplus spent. What it
 * is spent on, in order:
 *
 * 1. The battlefields, up to the card floor. Cards becoming chips is step 5 of the ladder and
 *    almost everything else is earlier, so the board buys its card faces back before the hand
 *    buys its designed size.
 * 2. The seats and the dock, up to their designed sizes. Both are small and both are read
 *    constantly.
 * 3. The dock again, when the game is asking. This is §2's trade, and it is *ordering* rather
 *    than a band: the controls take the bottom band before the hand grows past its floor, so a
 *    screen with room for both keeps both and a screen without one gives the room to the
 *    question that is waiting for an answer.
 * 4. The hand, up to its designed size. Its floor is deliberately larger than a permanent's,
 *    because the hand is where a player *chooses* and §6 forbids abbreviating a name there.
 * 5. The battlefields, up to designed and then past it. Whatever is left is board — the one
 *    region that is better for having more of it.
 *
 * Every region is charged at every step whether or not anything is in it, and the two
 * battlefields are charged as one item bought twice. That is what keeps a board wipe from
 * resizing the table under the player, and a first creature from shoving the other half of it
 * upward: the height of a field is what the viewport can afford, and the count inside it is the
 * cards' problem, not the box's (§5).
 *
 * The order is what makes this monotone: every step spends what is left over from the step before
 * it, so a pixel of extra height can only ever make a region bigger.
 */
function columnHeights(available: number, asking: boolean, handFloor: number): Column {
  let seat = SEAT.min
  let dock = DOCK.min
  let hand = handFloor
  let field = CHIP_ROW
  let left = available - (2 * seat + dock + hand + 2 * field)

  /** Spend what there is toward a target, shared equally between however many regions want it. */
  const raise = (current: number, target: number, copies: number): number => {
    if (copies === 0 || left <= 0 || target <= current) return current
    const got = Math.min((target - current) * copies, left)
    left -= got
    return current + Math.floor(got / copies)
  }

  /**
   * Give height back when even the floors do not fit.
   *
   * Unreachable from any supported viewport — the smallest of them, 320×480 and 480×320, both
   * clear the floors — and here because the invariant it defends is absolute: the boxes this
   * returns never exceed the viewport, whatever it is handed.
   */
  const lower = (current: number, floor: number, copies: number): number => {
    if (copies === 0 || left >= 0 || floor >= current) return current
    const given = Math.min((current - floor) * copies, -left)
    left += given
    return current - Math.ceil(given / copies)
  }

  hand = lower(hand, Math.min(hand, HAND_PEEK), 1)
  field = lower(field, 0, 2)
  hand = lower(hand, 0, 1)
  dock = lower(dock, 0, 1)
  seat = lower(seat, 0, 2)

  field = raise(field, fieldOf(CARD_ROW.min), 2)
  seat = raise(seat, SEAT.designed, 2)
  dock = raise(dock, DOCK.designed, 1)
  if (asking) dock = raise(dock, DOCK_ASKING, 1)
  hand = raise(hand, HAND.designed, 1)
  field = raise(field, fieldOf(CARD_ROW.designed), 2)
  if (left > 0) field += Math.floor(left / 2)

  return { seat, field, dock, hand }
}

/** The widths across the table: the rails at the edges, the side column, and the board between. */
interface Across {
  turn: number
  stack: number
  side: number
  board: number
  boardX: number
}

/** What the rails have been paid for, and what is set aside for the ones that are not drawn yet. */
interface Rails {
  /** Width the board may not have. Ramped, so the board is flat across a rail arriving, never cut. */
  reserved: number
  turn: number
  stack: number
}

/**
 * Which rails a width can afford — and, before it can, how much it has saved toward them.
 *
 * The saving is the point. A rail that simply appeared at the width it fits would take its whole
 * width off the board in one pixel of extra screen, which is the defect; a rail that is saved for
 * holds the board still for as long as it takes and then costs it nothing at all. What is saved
 * and not yet spent is gutter — up to a rail's width of it, and only in the band of widths where
 * a rail is about to arrive.
 *
 * Nothing here reads the height. A rail is a width question ("name the thing that does not fit",
 * §4), and a rail that came and went as the same screen got taller would move the board by two
 * hundred pixels in the direction §3 forbids.
 */
function railsFor(width: number): Rails {
  const reserved = Math.min(RAILS, Math.max(0, width - RAILS_FROM))
  return RAIL_STEPS.reduce<Rails>(
    (afforded, step) => (step.turn + step.stack <= reserved ? { reserved, ...step } : afforded),
    { reserved, turn: 0, stack: 0 },
  )
}

/**
 * Divide the width between the rails, the side column, and the board.
 *
 * The stack costs nothing at all when it is empty, which is §5's one exception and not a rule the
 * battlefields share: an event that is not happening takes no room, a place at the table keeps its
 * box. What the exception does not cover is depth, so the rail is the same width for one object as
 * for seven — and it does not cover the *arrangement* either, which is why the reserve is what it
 * is regardless and only the rail's own width comes back.
 *
 * The side column is last and is paid for out of what is left *after* the board has taken its
 * maximum. That single rule reproduces §4's table: at 1440 and 1920 there is nothing spare and
 * the side is a drawer, and at 3440 there is and it is a column.
 *
 * It is measured against the width the stack rail *would* take rather than the width it is taking
 * — the one place the stack's exception is deliberately not carried through. A stack that empties
 * gives its width back to the board, which is a board that grows; if it also promoted the side
 * panel from a drawer to a column, resolving a spell would rearrange the screen.
 */
function acrossWidths(width: number, rails: Rails, up: boolean): Across {
  const stack = up ? rails.stack : 0
  const spare = width - rails.reserved - BOARD_MAX
  const side = spare >= SIDE.min ? Math.min(SIDE.designed, spare) : 0
  const free = width - rails.turn - stack - side
  const board = Math.min(width - rails.reserved + (rails.stack - stack) - side, BOARD_MAX)
  return {
    turn: rails.turn,
    stack,
    side,
    board,
    boardX: rails.turn + Math.floor((free - board) / 2),
  }
}

/**
 * The whole arrangement, for one viewport and one set of counts.
 *
 * Fractional viewports — which is what a zoomed browser reports — are floored rather than
 * rounded, so the union of the boxes is inside the screen at every zoom level rather than one
 * subpixel past it.
 */
export function scene(viewport: Viewport, counts: SceneCounts): Scene {
  const width = Math.max(0, Math.floor(viewport.width))
  const height = Math.max(0, Math.floor(viewport.height))
  const band = bandOf(width, height)

  // Nothing to arrange. The caller draws §1's notice over the whole viewport, and every region
  // is empty rather than being given a box a board could be attempted in.
  if (band === 'unsupported') {
    return {
      band,
      regions: allEmpty(),
      ladder: { rails: 'collapsed', rows: 'merged', cardTier: 'chip', sidePanel: 'drawer' },
    }
  }

  const hasStack = counts.stackDepth > 0
  const rails = railsFor(width)
  const across = acrossWidths(width, rails, hasStack)
  // Where there is no turn rail the turn is a row under the header instead, and the stack — if
  // there is one and it has not yet earned a rail — is the edge badge §4 names, in the same row.
  const collapsed = rails.turn === 0
  const rowHeight = collapsed ? TURN_ROW : 0
  const badge = rowHeight > 0 && rails.stack === 0 && hasStack ? STACK_BADGE : 0

  const headerHeight = ramp(height, HEADER_FROM, HEADER.min, HEADER.designed)
  const contentTop = headerHeight + rowHeight
  const contentHeight = Math.max(0, height - contentTop)

  // The hand and the controls trade the bottom band (§2), and both halves of the trade are read
  // off the room rather than off a band: the hand's floor is the peek strip until the height past
  // §4's Short floor has bought it back, and the dock takes what it needs to ask a question before
  // the hand grows past that floor. It is the asking that moves it and never the number of cards —
  // an empty hand is still a place, drawn at the height the hand it is about to hold will need.
  const handFloor = ramp(height, HAND_FROM, HAND_PEEK, HAND.min)
  const column = columnHeights(contentHeight, counts.asking === true, handFloor)

  const box = (y: number, regionHeight: number): Rect =>
    regionHeight <= 0 ? EMPTY : { x: across.boardX, y, width: across.board, height: regionHeight }

  // Down from the top for the opponent's half and up from the bottom for yours, so the hand sits
  // on the bottom edge and the odd pixel an even split cannot place falls on the line between the
  // two fields, which is the one seam in the table where nothing is read.
  const opponentSeatY = contentTop
  const opponentFieldY = opponentSeatY + column.seat
  const handY = contentTop + contentHeight - column.hand
  const dockY = handY - column.dock
  const yourSeatY = dockY - column.seat
  const yourFieldY = yourSeatY - column.field

  const regions: Record<RegionName, Rect> = {
    header: { x: 0, y: 0, width, height: headerHeight },
    turn:
      rowHeight > 0
        ? { x: 0, y: headerHeight, width: width - rails.reserved - badge, height: rowHeight }
        : { x: 0, y: contentTop, width: across.turn, height: contentHeight },
    stack:
      badge > 0
        ? { x: width - badge, y: headerHeight, width: badge, height: rowHeight }
        : across.stack === 0
          ? EMPTY
          : { x: width - across.stack, y: contentTop, width: across.stack, height: contentHeight },
    side:
      across.side === 0
        ? EMPTY
        : {
            x: width - across.stack - across.side,
            y: contentTop,
            width: across.side,
            height: contentHeight,
          },
    opponentSeat: box(opponentSeatY, column.seat),
    opponentField: box(opponentFieldY, column.field),
    yourField: box(yourFieldY, column.field),
    yourSeat: box(yourSeatY, column.seat),
    dock: box(dockY, column.dock),
    hand: box(handY, column.hand),
  }

  return { band, regions, ladder: ladderFor(collapsed, column.field, across.side > 0) }
}

/**
 * Which steps of §3 the allocation ended up needing.
 *
 * Both answers are read off the height a battlefield got — a height both fields got, so there is
 * one answer here and not one per seat — and **both are measured against the bottom of the ladder
 * rather than against §5's review threshold.** That is the correction #685 forces: a row under
 * 100px is not a chip row, it is a row of smaller cards, so a threshold written at 100 gives up a
 * step of the ladder a whole tier early.
 *
 * **Rows merge when a row could no longer set a name.** Not when the rows would be small — the
 * split is what a board is read by, and §5 is explicit that the row count does not fall to buy
 * card size. It falls when there is nothing left to fall back on, which is a field that cannot
 * give every row the height a 9px name needs, gaps included.
 *
 * **The field is a chip board when the field itself cannot hold a card at §5's minimum.** Below
 * that the whole region is the bottom of the ladder and a landscape chip carries a name better
 * than the portrait tile the height affords — which is precisely why §5's chip is landscape. Above
 * it the scene says nothing about the tier and the *row* decides, through `presentationFor` on the
 * box it actually got: no surface names its own tier (§6), and the scene is a surface here.
 *
 * A merged field is a crowded field by definition, which is exactly §6's Compact presentation —
 * so the designed presentation is reserved for a board that still has its rows, and with three of
 * them it is a board no supported viewport is tall enough to buy. That is the honest answer rather
 * than a shortfall: 1440p draws three rows of 162px tiles, and `presentationFor` still calls a
 * 116×162 tile designed, because what a *card* is comes from its box and never from this word.
 *
 * Nothing here consults a count, so the flags describe the room unconditionally: an empty table
 * reports the arrangement it will still be in once both seats are full.
 */
function ladderFor(collapsed: boolean, field: number, sideColumn: boolean): SceneLadder {
  const rows = field >= fieldOf(CARD_MIN) ? 'split' : 'merged'
  const rowHeight =
    rows === 'split' ? Math.floor((field - (SPLIT_ROWS - 1) * ROW_GAP) / SPLIT_ROWS) : field
  return {
    rails: collapsed ? 'collapsed' : 'full',
    rows,
    cardTier:
      field < CARD_ROW.min
        ? 'chip'
        : rows === 'merged' || rowHeight < CARD_ROW.designed
          ? 'compact'
          : 'designed',
    sidePanel: sideColumn ? 'column' : 'drawer',
  }
}

function allEmpty(): Record<RegionName, Rect> {
  return {
    header: EMPTY,
    turn: EMPTY,
    opponentSeat: EMPTY,
    opponentField: EMPTY,
    stack: EMPTY,
    yourField: EMPTY,
    yourSeat: EMPTY,
    dock: EMPTY,
    hand: EMPTY,
    side: EMPTY,
  }
}
