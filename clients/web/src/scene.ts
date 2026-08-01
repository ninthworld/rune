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
 * Two places where the spec's own sections point in different directions, and what was done:
 *
 * - §4 gives the Wide band as "ratio ≥ 1.3, height ≥ 640", which leaves 853×480 — a 1280×720
 *   desktop at 150% zoom — in no band at all. The height in that row describes where the
 *   *designed* sizes survive, and that is now something the allocation below answers rather than
 *   something a band asserts, so the band is chosen by ratio alone once the Short floor is past.
 * - §4's Tall row says rows merge and §3 puts merging *after* chips. Merging is what keeps a
 *   permanent a card on a phone — one row of 163px beats two of 81px — so it is applied first,
 *   and by the room a field actually got rather than by the band. A 844px-tall phone keeps its
 *   rows split; a 667px one merges them; both keep card faces, which is what §4 promises.
 */

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
   * turns for it (§2): the hand is full height while nothing is pending, and collapses to a peek
   * strip the moment there is something to answer, with the dock taking what it freed. Restoring
   * the hand over that is a gesture, and a hand raised over a peek strip is an overlay rather
   * than a region — the same class of thing as an opened pile — because the space it would need
   * belongs to tier-1 minimums the board cannot give up.
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
  /** Step 7. `collapsed` means the turn is one current-step chip and the stack is a badge. */
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
/** The turn as a vertical rail: twelve steps, each of them the control that sets a stop there. */
const TURN_RAIL: Size = { min: 88, designed: 112 }
/** The same twelve steps laid horizontally under the header, where the ratio is square. */
const TURN_STRIP = 32
/** Collapsed (§3, step 7): the current step, and nothing else. */
const TURN_CHIP = 28
/** A stack item is 130 wide designed and a chip is 96 (§5); the rail is that plus its gutter. */
const STACK_RAIL: Size = { min: 104, designed: 150 }
/** Collapsed: the top item's name and a count, which §3 says never degrades further. */
const STACK_BADGE = 132
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
/** §5's chip is 96×30. A field that has gone to chips still draws one row of them. */
const CHIP_ROW = 36
/** §5's permanent tile: 72×100 at the floor — which is §3 step 5's threshold — and 130×182. */
const CARD_ROW: Size = { min: 100, designed: 182 }
/** Preview, log, and settle as a column. Below its floor it is a drawer instead (§3, step 8). */
const SIDE: Size = { min: 260, designed: 320 }
/** §4, Ultrawide: the board's content is held to a width a glance does not have to travel. */
const BOARD_MAX = 1440
/** The width the board keeps whatever else happens: a rail falls back to its floor before this does. */
const BOARD_MIN = 320
/**
 * How many rows a split battlefield is budgeted for.
 *
 * `board.ts` produces up to three — creatures, other permanents, lands — and the third is the
 * exception rather than the rule. Budgeting every field for it would spend a third of every
 * battlefield on a row most boards never draw, and it would put §4's own promise out of reach:
 * a portrait phone cannot keep card faces if its field is cut three ways.
 */
const SPLIT_ROWS = 2

const EMPTY: Rect = { x: 0, y: 0, width: 0, height: 0 }

/**
 * Which band a viewport is in.
 *
 * Ordered by what is scarce rather than by ratio alone: below the floor there is no arrangement
 * to state, and below 480px of height the scarce resource is height no matter how wide the
 * screen is, which is the whole of what the Short band means. Everything past that is the ratio,
 * and the thresholds are §4's.
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
 * 3. The hand, up to its designed size. Its floor is deliberately larger than a permanent's,
 *    because the hand is where a player *chooses* and §6 forbids abbreviating a name there.
 * 4. The dock again, when the game is asking. This is the trade §2 describes and it is funded by
 *    the hand that collapsed, which is why it is spent after the hand and not before it.
 * 5. The battlefields, up to designed and then past it. Whatever is left is board — the one
 *    region that is better for having more of it.
 *
 * Every region is charged at every step whether or not anything is in it, and the two
 * battlefields are charged as one item bought twice. That is what keeps a board wipe from
 * resizing the table under the player, and a first creature from shoving the other half of it
 * upward: the height of a field is what the viewport can afford, and the count inside it is the
 * cards' problem, not the box's (§5).
 */
function columnHeights(available: number, asking: boolean, peekHand: boolean): Column {
  const handTarget = peekHand ? HAND_PEEK : HAND.designed

  let seat = SEAT.min
  let dock = DOCK.min
  let hand = peekHand ? HAND_PEEK : HAND.min
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

  hand = lower(hand, peekHand ? 0 : HAND_PEEK, 1)
  field = lower(field, 0, 2)
  hand = lower(hand, 0, 1)
  dock = lower(dock, 0, 1)
  seat = lower(seat, 0, 2)

  field = raise(field, SPLIT_ROWS * CARD_ROW.min, 2)
  seat = raise(seat, SEAT.designed, 2)
  dock = raise(dock, DOCK.designed, 1)
  hand = raise(hand, handTarget, 1)
  if (asking) dock = raise(dock, DOCK_ASKING, 1)
  field = raise(field, SPLIT_ROWS * CARD_ROW.designed, 2)
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

/**
 * Divide the width, for the arrangements that have rails in them.
 *
 * The stack is taken first because it is tier 1 whenever it is non-empty — and costs nothing at
 * all when it is empty, which is §5's one exception and not a rule the battlefields share: an
 * event that is not happening takes no room, a place at the table keeps its box. What the
 * exception does not cover is depth, so the rail is the same width for one object as for seven.
 * The turn rail is next, both falling back to their floors rather than to nothing, because the
 * board keeps `BOARD_MIN` whatever else happens.
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
function acrossWidths(width: number, railWidth: number, stackCeiling: number, up: boolean): Across {
  const reserved =
    stackCeiling === 0 ? 0 : fit(STACK_RAIL, width - railWidth - BOARD_MIN, stackCeiling)
  const stack = up ? reserved : 0
  const turn = railWidth === 0 ? 0 : fit(TURN_RAIL, width - reserved - BOARD_MIN, railWidth)
  const free = width - stack - turn
  const spare = width - turn - reserved - BOARD_MAX
  const side = spare >= SIDE.min ? Math.min(SIDE.designed, spare) : 0
  const board = Math.min(free - side, BOARD_MAX)
  return { turn, stack, side, board, boardX: turn + Math.floor((free - side - board) / 2) }
}

/** A rail at its designed width where the budget allows, at its floor where it does not. */
const fit = (size: Size, budget: number, ceiling: number): number =>
  Math.min(ceiling, budget >= size.designed ? size.designed : size.min)

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

  const collapsed = band === 'tall' || band === 'short'
  // A square viewport lays the turn out horizontally under the header, and the stack becomes the
  // edge tab §4 names — sized to the rail's *floor*, because the top item is named there and §3
  // says a name on the stack is one of the things that never degrades.
  const stripped = band === 'square'
  const headerHeight = height >= 640 ? HEADER.designed : HEADER.min
  const hasStack = counts.stackDepth > 0

  const across = collapsed
    ? { turn: 0, stack: 0, side: 0, board: Math.min(width, BOARD_MAX), boardX: 0 }
    : acrossWidths(
        width,
        stripped ? 0 : TURN_RAIL.designed,
        stripped ? STACK_RAIL.min : STACK_RAIL.designed,
        hasStack,
      )

  const badge = collapsed && hasStack ? Math.min(STACK_BADGE, Math.floor(width / 2)) : 0
  const stripHeight = collapsed ? TURN_CHIP : stripped ? TURN_STRIP : 0
  const contentTop = headerHeight + stripHeight
  const contentHeight = Math.max(0, height - contentTop)

  // The hand yields the bottom band to the controls where the two of them cannot both have it:
  // always at Short, where height is the scarce resource and §4 makes the peek strip the resting
  // state, and at Tall only while something is actually being asked. It is the asking that moves
  // it and never the number of cards — an empty hand is still a place, and is drawn at the height
  // the hand it is about to hold will need.
  const peekHand = band === 'short' || (band === 'tall' && counts.asking === true)
  const column = columnHeights(contentHeight, counts.asking === true, peekHand)

  const boardX = collapsed ? Math.floor((width - across.board) / 2) : across.boardX
  const box = (y: number, regionHeight: number): Rect =>
    regionHeight <= 0 ? EMPTY : { x: boardX, y, width: across.board, height: regionHeight }

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
    turn: collapsed
      ? { x: 0, y: headerHeight, width: width - badge, height: TURN_CHIP }
      : stripped
        ? { x: 0, y: headerHeight, width, height: TURN_STRIP }
        : { x: 0, y: contentTop, width: across.turn, height: contentHeight },
    stack: collapsed
      ? badge === 0
        ? EMPTY
        : { x: width - badge, y: headerHeight, width: badge, height: TURN_CHIP }
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
 * Rows merge before faces become chips, and both are decided by the height a battlefield got —
 * a height both of them got, so there is one answer here and not one per seat. A field with room
 * for two rows of card faces keeps them apart, because a row is how a board is read by *where
 * things are*; a field without that room is better as one row of taller cards than as two rows
 * of chips, since §4's rule is that height decides whether a permanent is a card at all.
 *
 * A merged field is a crowded field by definition, which is exactly §6's Compact presentation —
 * so the designed presentation is reserved for a board that still has its rows.
 *
 * Nothing here consults a count, so the flags describe the room unconditionally: an empty table
 * reports the arrangement it will still be in once both seats are full.
 */
function ladderFor(collapsed: boolean, field: number, sideColumn: boolean): SceneLadder {
  const rows = field >= SPLIT_ROWS * CARD_ROW.min ? 'split' : 'merged'
  const rowHeight = rows === 'split' ? Math.floor(field / SPLIT_ROWS) : field
  return {
    rails: collapsed ? 'collapsed' : 'full',
    rows,
    cardTier:
      rowHeight < CARD_ROW.min
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
