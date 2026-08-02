import { describe, expect, it } from 'vitest'

import {
  fieldSlots,
  packField,
  packRow,
  type Box,
  type FieldOptions,
  type PackedRow,
  type PackOptions,
} from './pack'
import { scene } from './scene'

/** §5's numbers, restated here so a change to one of them fails a test rather than passing. */
const CARD_FLOOR = { width: 72, height: 100 }
const CARD_DESIGNED = { width: 130, height: 182 }
const CHIP = { width: 96, height: 30 }
const RATIO = 63 / 88

/** A field the size of a comfortable desktop's: wide, and two rows of card faces tall. */
const DESK: Box = { width: 1400, height: 229 }

/** Counts a real board reaches, and a few past it. */
const COUNTS = [1, 2, 3, 4, 6, 8, 10, 12, 15, 20, 25, 30, 40]

/** Where the last tile of a row ends. The whole contract in one number. */
const rightEdge = (packed: PackedRow): number =>
  packed.positions.reduce((most, at) => Math.max(most, at.x + packed.width), 0)

const bottomEdge = (packed: PackedRow): number =>
  packed.positions.reduce((most, at) => Math.max(most, at.y + packed.height), 0)

describe('one row, packed', () => {
  /**
   * The clamp, from the top: while there is room, a tile is the size the row's height affords —
   * never wider than §5's designed card and never wider than the printed proportion allows.
   */
  it('draws the designed card while the room allows it', () => {
    const packed = packRow({ width: 1400, height: 200 }, 4)
    expect(packed.width).toBe(CARD_DESIGNED.width)
    expect(packed.height).toBe(CARD_DESIGNED.height)
    expect(packed.overlapped).toBe(false)
  })

  /**
   * Height is what runs out on a table, so it decides the width — a row with 1400px going spare
   * and 114px of height still draws an 81px card, because the alternative is a card lying half
   * in the row below it.
   */
  it('takes the width from the row’s height, not from the room going spare', () => {
    const packed = packRow({ width: 1400, height: 114 }, 4)
    expect(packed.height).toBeLessThanOrEqual(114)
    expect(packed.width).toBe(Math.floor(114 * RATIO))
    expect(Math.abs(packed.width / packed.height - RATIO)).toBeLessThan(0.02)
  })

  /** §5's `fitted`: past a certain count the cards shrink, and the row still fits exactly. */
  it('shrinks the cards toward the floor as the count grows', () => {
    const four = packRow(DESK, 4).width
    const twelve = packRow(DESK, 12).width
    expect(twelve).toBeLessThan(four)
    expect(packRow(DESK, 12).overlapped).toBe(false)
  })

  /**
   * The switchover, which is the sentence §5 is written to make true: **cards never shrink below
   * the floor — they overlap instead.**
   */
  it('overlaps rather than shrinking past the floor', () => {
    const packed = packRow({ width: 600, height: 229 }, 20)
    expect(packed.width).toBeGreaterThanOrEqual(CARD_FLOOR.width)
    expect(packed.overlapped).toBe(true)
    expect(packed.pitch).toBeLessThan(packed.width)
    expect(rightEdge(packed)).toBeLessThanOrEqual(600)
  })

  /** And the pitch it overlaps at is §5's, to the pixel the rounding allows. */
  it('fans at the pitch §5 states', () => {
    const packed = packRow({ width: 600, height: 120 }, 20)
    expect(packed.lines).toBe(1)
    expect(packed.pitch).toBeCloseTo((600 - packed.width) / 19, 5)
  })

  /**
   * §3's step 6, and §4's one evaluable rule: a permanent is a card while its row is at least
   * 100px tall, and a landscape chip below that. Rendering a 60px "card" instead is a shape with
   * the outline of information and none of the substance.
   */
  it('draws chips below a 100px row and cards at it', () => {
    expect(packRow({ width: 600, height: 99 }, 4).tier).toBe('chip')
    expect(packRow({ width: 600, height: 99 }, 4).height).toBe(CHIP.height)
    expect(packRow({ width: 600, height: 99 }, 4).width).toBe(CHIP.width)
    expect(packRow({ width: 600, height: 100 }, 4).tier).not.toBe('chip')
    expect(packRow({ width: 600, height: 100 }, 4).width).toBeGreaterThanOrEqual(CARD_FLOOR.width)
  })

  /** The ladder can say chip before the height does. It is never contradicted the other way. */
  it('draws chips when the ladder says so', () => {
    expect(packRow({ width: 600, height: 140 }, 4, { chip: true }).tier).toBe('chip')
    expect(packRow({ width: 600, height: 140 }, 4, { chip: true }).height).toBe(CHIP.height)
  })

  /** A chip is already the floor: it never shrinks, so a crowded chip row fans instead. */
  it('never shrinks a chip', () => {
    for (const count of COUNTS) {
      const packed = packRow({ width: 500, height: 60 }, count, { chip: true })
      expect(packed.width).toBe(CHIP.width)
      expect(rightEdge(packed)).toBeLessThanOrEqual(500)
    }
  })

  /**
   * The bound §5 puts under the pitch, spent the only way a row can spend it: twenty permanents
   * in a field sized for eight take a second line rather than fanning, and every tile stays whole
   * at the size that line affords.
   */
  it('takes a second line before it fans, where the height is going spare', () => {
    const packed = packRow({ width: 1170, height: 229 }, 20)
    expect(packed.lines).toBe(2)
    expect(packed.overlapped).toBe(false)
    expect(packed.width).toBeGreaterThan(CARD_FLOOR.width)
    expect(bottomEdge(packed)).toBeLessThanOrEqual(229)
    // The line it bought was paid for out of height, never out of the tile: both lines hold a
    // card face, which is what keeps this from being §3's step 6 taken early.
    expect(packed.height).toBeGreaterThanOrEqual(CARD_FLOOR.height)
  })

  it('wraps a crowded chip row rather than fanning it past a legible strip', () => {
    const packed = packRow({ width: 500, height: 70 }, 20, { chip: true })
    expect(packed.lines).toBe(2)
    expect(packed.pitch).toBeGreaterThan(30)
    expect(bottomEdge(packed)).toBeLessThanOrEqual(70)
  })

  /**
   * And never at the cost of the tile. A row whose height is fully spent on one line of cards
   * fans instead of wrapping — a second line paid for by shorter tiles would be step 6 taken
   * early, and the card is what the ladder protects longest.
   */
  it('never buys a line with the height a card face needs', () => {
    const tight = packRow({ width: 400, height: 120 }, 20)
    expect(tight.lines).toBe(1)
    expect(tight.overlapped).toBe(true)
    expect(tight.height).toBeGreaterThanOrEqual(CARD_FLOOR.height)

    // The same count in a field with height going spare — the cards are at the floor and the row
    // is more than twice as tall as they are — takes the line rather than the fan.
    const roomy = packRow({ width: 400, height: 229 }, 20)
    expect(roomy.lines).toBe(2)
    expect(roomy.height).toBe(tight.height)
    expect(roomy.pitch).toBeGreaterThan(tight.pitch)
  })

  /**
   * The invariant the whole file exists for, at every count a board reaches and every box the
   * supported range produces: **nothing ever leaves the box.** There is no overflow to fall
   * through to, so a row that did not fit would be a row drawn over the seat bar under it.
   */
  it('keeps every tile inside the box, at every count and every size', () => {
    for (const width of [320, 500, 640, 1000, 1400, 3000]) {
      for (const height of [30, 60, 99, 100, 114, 160, 229, 400]) {
        for (const count of COUNTS) {
          const packed = packRow({ width, height }, count)
          expect(rightEdge(packed)).toBeLessThanOrEqual(width)
          expect(bottomEdge(packed)).toBeLessThanOrEqual(height)
          expect(packed.positions).toHaveLength(count)
          expect(packed.positions.every((at) => at.x >= 0 && at.y >= 0)).toBe(true)
        }
      }
    }
  })

  /** The printed proportion, at every size a card tier produces (§5). */
  it('keeps 63:88 wherever it is still drawing a card', () => {
    for (const height of [100, 114, 160, 182, 229, 400]) {
      for (const count of COUNTS) {
        const packed = packRow({ width: 1400, height }, count)
        if (packed.tier === 'chip') continue
        expect(Math.abs(packed.width / packed.height - RATIO)).toBeLessThan(0.03)
      }
    }
  })

  /** A row of one has no pitch to state, and a row of none has nothing to place. */
  it('answers for the degenerate counts', () => {
    // Centred in the height it did not need, and at the start of the row: the server's order
    // reads from the same edge whether one permanent is on the board or twenty.
    expect(packRow(DESK, 1).positions).toEqual([{ x: 0, y: 23 }])
    expect(packRow(DESK, 1).overlapped).toBe(false)
    expect(packRow(DESK, 0).positions).toEqual([])
    expect(packRow({ width: 0, height: 0 }, 4).positions).toHaveLength(4)
  })
})

describe('a field, divided into rows', () => {
  const SPLIT: PackOptions = { slots: 2, cardTier: 'compact' }

  it('gives every row the same height and none of them the field’s own edge', () => {
    const plan = packField(DESK, [4, 2], SPLIT)
    expect(plan.rows).toHaveLength(2)
    expect(plan.rows[0]?.height).toBe(plan.rows[1]?.height)
    expect(plan.rows[0]?.y).toBe(0)
    expect(plan.rows[1]!.y + plan.rows[1]!.height).toBeLessThanOrEqual(DESK.height)
    expect(plan.rows.every((row) => row.x + row.width <= DESK.width)).toBe(true)
  })

  /**
   * The scene funds two rows of exactly the card floor, so a gap taken off the top of that is
   * two rows a few pixels under it — a board of cards turned into a board of chips over three
   * pixels of air. §3 compresses gaps (step 1) long before it gives up a face (step 6).
   */
  it('compresses the gap between rows rather than the cards in them', () => {
    const plan = packField({ width: 1400, height: 200 }, [4, 2], SPLIT)
    expect(plan.rows[0]?.height).toBe(CARD_FLOOR.height)
    expect(plan.rows[0]?.pack.tier).not.toBe('chip')
    expect(plan.rows[1]?.y).toBe(CARD_FLOOR.height)
  })

  /**
   * A half with fewer groups than the table draws its rows at the table's height and leaves the
   * far slot empty — the price of a permanent being the same size at both ends of the table, and
   * paid at the edge away from the dividing line where the absence is what it reads as.
   */
  it('keeps the table’s row height on a half with fewer rows', () => {
    const yours = packField(DESK, [4], SPLIT)
    const theirs = packField(DESK, [4], { ...SPLIT, mirrored: true })
    expect(yours.rows[0]?.height).toBe(packField(DESK, [4, 2], SPLIT).rows[0]?.height)
    expect(yours.rows[0]?.pack.width).toBe(packField(DESK, [4, 2], SPLIT).rows[0]?.pack.width)
    // Yours fills from the dividing line down; theirs is mirrored, so it fills upward from it.
    expect(yours.rows[0]?.y).toBe(0)
    expect(theirs.rows[0]?.y).toBeGreaterThan(0)
  })

  /** An empty group is not among the counts and costs no row. */
  it('draws no row for a group that is not there', () => {
    expect(packField(DESK, [6], { slots: 1, cardTier: 'compact' }).rows).toHaveLength(1)
    expect(packField(DESK, [], SPLIT).rows).toEqual([])
  })

  it('packs a merged field as one row of everything', () => {
    const plan = packField(DESK, [7], { slots: 1, cardTier: 'compact' })
    expect(plan.rows).toHaveLength(1)
    expect(plan.rows[0]?.height).toBe(DESK.height)
    expect(plan.rows[0]?.pack.width).toBe(CARD_DESIGNED.width)
  })

  it('draws chips throughout a field the ladder chipped', () => {
    const plan = packField({ width: 620, height: 60 }, [12], { slots: 1, cardTier: 'chip' })
    expect(plan.rows[0]?.pack.tier).toBe('chip')
    expect(plan.rows[0]?.pack.height).toBe(CHIP.height)
  })
})

describe('how many rows a table draws', () => {
  const SPLIT: FieldOptions = { rows: 'split', cardTier: 'compact' }
  /** A half with this many groups, uncrowded: two permanents in each, in a 1400px row. */
  const half = (groups: number): number[] => Array.from({ length: groups }, () => 2)
  /** The table, said as how many groups each half has. Crowding is a separate case below. */
  const AT = (height: number, groups: number[], tier: FieldOptions['cardTier'] = 'compact') =>
    fieldSlots({ width: 1400, height }, groups.map(half), { rows: 'split', cardTier: tier })
  /** `Battlefield.tsx`'s collapse, as counts: the groups furthest from the middle merge first. */
  const collapse = (counts: readonly number[], rows: number): readonly number[] =>
    counts.length <= rows
      ? counts
      : [...counts.slice(0, rows - 1), counts.slice(rows - 1).reduce((all, one) => all + one, 0)]
  /** The smallest card on a packed field, which is what a board's readability is. */
  const smallest = (plan: { rows: readonly { pack: PackedRow }[] }): number =>
    plan.rows.reduce((least, row) => Math.min(least, row.pack.width), Infinity)

  /** The height at which two rows and one row draw the same card: 2 × 182, and the gap between. */
  const TWO_ROWS = 2 * CARD_DESIGNED.height + 6
  /** And three: 3 × 182, and two gaps. */
  const THREE_ROWS = 3 * CARD_DESIGNED.height + 12

  /**
   * The split is kept while it is *free* — while the rows it makes still draw the same card the
   * merged row would. Rows count for exactly one thing and it is this tie-break, because the scan
   * by category costs nothing here.
   */
  it('gives the table every row it can draw without shrinking the card', () => {
    expect(AT(TWO_ROWS, [2, 2])).toBe(2)
    expect(AT(TWO_ROWS, [3, 2])).toBe(2)
    expect(AT(THREE_ROWS, [3, 2])).toBe(3)
    expect(AT(THREE_ROWS, [2, 2])).toBe(2)
    expect(AT(400, [1, 1])).toBe(1)
  })

  /**
   * And merges the moment the split would cost card size, which is §3's "More screen is never a
   * worse board" applied to the one decision that can violate it. **This is not a floor test.**
   * Three rows of 100px-and-a-bit each clear the card floor and are still the wrong answer: the
   * same height merged draws 130×182 cards with complete names, type lines and rules text, and
   * one row of those beats three rows of clipped 75px ones. Row count maximises card size, not
   * rows.
   */
  it('merges as soon as a row would draw a smaller card', () => {
    expect(AT(TWO_ROWS - 1, [2, 2])).toBe(1)
    expect(AT(THREE_ROWS - 1, [3, 2])).toBe(2)
    // The field that produced the inversion this rule was written for: 319px at 1920×1080, where
    // three rows each clear the 100px floor and each draw a 73px card.
    expect(AT(319, [3, 2])).toBe(1)
    expect(AT(229, [3, 2])).toBe(1)
    expect(AT(199, [2, 2])).toBe(1)
  })

  /**
   * And on the count as much as on the height, because a row's tile is decided by both.
   *
   * A 390px field with just enough height for two rows of designed cards, and twelve permanents in
   * three groups: split, one row holds eight and draws them at the 72px floor; merged, the same
   * height spreads all twelve over three lines of 88px cards. Every row of the split clears the
   * card floor and is tall enough for a designed card — a rule written on the height alone would
   * take it — and it is still the smaller card, so it is still the wrong answer.
   */
  it('merges where the split’s own rows would be the crowded ones', () => {
    const table = [
      [4, 4, 4],
      [4, 4, 4],
    ]
    expect(fieldSlots({ width: 390, height: 370 }, table, SPLIT)).toBe(1)
    // The same board with room to spread out keeps every row it can.
    expect(fieldSlots({ width: 1400, height: 370 }, table, SPLIT)).toBe(2)
  })

  /**
   * The invariant under all of it, and the reason the property below holds by construction: **the
   * card the table draws is the card the merged row would have drawn**, whatever row count came
   * out. A merged row can reproduce any split — the same line height, with the count spread evenly
   * instead of by category — so merging is never the smaller card, and a split is only ever taken
   * where it matches it exactly.
   */
  it('never trades card size for a row', () => {
    for (const width of [390, 640, 1000, 1400, 3000]) {
      for (const height of [100, 150, 199, 229, 300, 319, 370, 499, 560, 800]) {
        for (const board of [
          [2, 2],
          [4, 3, 1],
          [8, 8, 8],
          [1, 1, 18],
          [20, 1],
        ]) {
          const box = { width, height }
          const slots = fieldSlots(box, [board, board], SPLIT)
          const total = board.reduce((all, one) => all + one, 0)
          const merged = packField(box, [total], { slots: 1, cardTier: 'compact' })
          const chosen = packField(box, collapse(board, slots), { slots, cardTier: 'compact' })
          expect(smallest(chosen)).toBe(smallest(merged))
        }
      }
    }
  })

  /** Answered for the table, so the busiest half decides for both of them. */
  it('answers once for every half', () => {
    expect(AT(229, [3, 1])).toBe(AT(229, [1, 3]))
    expect(AT(400, [3, 1])).toBe(AT(400, [1, 3]))
  })

  /** A chip row is past the point where a face is at stake, so it keeps its rows for longer. */
  it('holds a chipped table to the chip’s own floor', () => {
    expect(AT(90, [2, 2], 'chip')).toBe(2)
    expect(AT(50, [2, 2], 'chip')).toBe(1)
  })

  /** The scene's own answer is never overturned. */
  it('cannot un-merge what the ladder merged', () => {
    expect(
      fieldSlots(
        { width: 1400, height: 400 },
        [
          [2, 2, 2],
          [2, 2],
        ],
        { rows: 'merged', cardTier: 'compact' },
      ),
    ).toBe(1)
  })
})

/**
 * The packing against the scene it is fed by, at the whole supported range.
 *
 * The two modules are pure and the seam between them is a box, so the promise §5 makes — a count
 * is absorbed by the cards, never by the region — is checkable end to end without a browser: the
 * same field, at counts from one permanent to forty, and nothing off the edge of any of them.
 */
describe('every supported viewport, at every count', () => {
  const VIEWPORTS = [
    { width: 3440, height: 1440 },
    { width: 1920, height: 1080 },
    { width: 1440, height: 900 },
    { width: 1280, height: 720 },
    { width: 1000, height: 1000 },
    { width: 844, height: 390 },
    { width: 640, height: 360 },
    { width: 390, height: 844 },
    { width: 320, height: 480 },
  ]

  it('fits every permanent inside the field the scene gave it', () => {
    for (const viewport of VIEWPORTS) {
      const { regions, ladder } = scene(viewport, { stackDepth: 1 })
      for (const count of COUNTS) {
        const slots = fieldSlots(
          regions.yourField,
          [
            [2, 2, 2],
            [2, 2],
          ],
          ladder,
        )
        const groups = slots === 1 ? [count] : [count - 1, 1]
        const plan = packField(regions.yourField, groups, { slots, cardTier: ladder.cardTier })
        for (const row of plan.rows) {
          expect(row.y + row.height).toBeLessThanOrEqual(regions.yourField.height)
          expect(rightEdge(row.pack)).toBeLessThanOrEqual(row.width)
          expect(bottomEdge(row.pack)).toBeLessThanOrEqual(row.height)
        }
      }
    }
  })

  /** And no tile is ever below the floor of the tier it is drawing. */
  it('never draws a tile under §5’s floor for its tier', () => {
    for (const viewport of VIEWPORTS) {
      const { regions, ladder } = scene(viewport, { stackDepth: 1 })
      for (const count of COUNTS) {
        const plan = packField(regions.yourField, [count], {
          slots: 1,
          cardTier: ladder.cardTier,
        })
        const packed = plan.rows[0]?.pack
        if (!packed) continue
        if (packed.tier === 'chip') expect(packed.width).toBe(CHIP.width)
        else expect(packed.width).toBeGreaterThanOrEqual(CARD_FLOOR.width)
      }
    }
  })
})

/**
 * §3, "More screen is never a worse board", asserted as the property it is stated as.
 *
 * **For a fixed board, a card is never smaller on a larger viewport than on a smaller one.** It
 * is checked by sweeping the supported range rather than by listing viewports and their expected
 * sizes, and that is the whole point of writing it this way: a table of per-size expectations is
 * exactly what was in this file when 1440×900 drew one row of 130×182 cards and 1920×1080 —
 * a *larger* screen, same board — drew three rows of 75px ones. Every number in that table was
 * green. A sweep compares sizes to each other instead of to a constant, so it fails on the
 * relationship rather than on a value somebody has to know in advance.
 *
 * The whole pipeline is swept, not `packRow` alone: the scene's field, the row count the table
 * chose, the collapse a half does when it has more groups than the table draws, and the packing
 * inside the row. The inversion lived in the second of those and was invisible in every one of
 * the others.
 */
describe('more screen is never a worse board', () => {
  /** Boards a real game reaches, grouped the way `board.ts` groups them. Fixed as the screen moves. */
  const BOARDS = [1, 5, 12, 20, 40]

  /** Creatures, other permanents, lands — as evenly as that many permanents divide. */
  const boardOf = (permanents: number): readonly number[] => {
    const rows = Math.max(1, Math.min(3, permanents))
    return Array.from({ length: rows }, (_, index) =>
      Math.floor((permanents + rows - 1 - index) / rows),
    )
  }

  /**
   * How the two tiles compare, negative when the first is the worse board.
   *
   * By tier first and width second, because a chip is not a narrow card: going from a chip to a
   * card is an improvement even where the card is narrower than the 96px chip, and going the
   * other way is the ladder's step 6 and a loss however wide the chip is. Comparing widths
   * alone would call `320×480 → 390×844` — chips becoming 72px card faces — a regression.
   */
  const rank = (packed: PackedRow): number => (packed.tier === 'chip' ? 0 : 1)
  const compare = (a: PackedRow, b: PackedRow): number => rank(a) - rank(b) || a.width - b.width
  const say = (packed: PackedRow): string =>
    `${packed.width}×${packed.height} ${packed.tier}${packed.lines > 1 ? ` ×${packed.lines}` : ''}`

  /**
   * What one viewport draws for one board — the *worst* row of it, since a board is only as
   * readable as the smallest card on it. `undefined` where the viewport is below §1's floor and
   * there is no board to draw at all.
   */
  const tileIn = (
    field: Box,
    board: readonly number[],
    ladder: FieldOptions,
  ): PackedRow | undefined => {
    if (field.width <= 0 || field.height <= 0) return undefined
    const slots = fieldSlots(field, [board, board], ladder)
    // `Battlefield.tsx`'s collapse: the rows furthest from the dividing line merge first.
    const counts =
      board.length <= slots
        ? board
        : [
            ...board.slice(0, slots - 1),
            board.slice(slots - 1).reduce((sum, count) => sum + count, 0),
          ]
    const plan = packField(field, counts, { slots, cardTier: ladder.cardTier })
    return plan.rows.reduce<PackedRow | undefined>(
      (worst, row) => (worst === undefined || compare(row.pack, worst) < 0 ? row.pack : worst),
      undefined,
    )
  }

  const tileAt = (
    viewport: { width: number; height: number },
    board: readonly number[],
  ): { tile: PackedRow; field: Box } | undefined => {
    const { regions, ladder } = scene(viewport, { stackDepth: 1 })
    const tile = tileIn(regions.yourField, board, ladder)
    return tile && { tile, field: regions.yourField }
  }

  /** Every width and height in the supported range, at four-pixel steps. */
  const WIDTHS = Array.from({ length: (3440 - 320) / 4 + 1 }, (_, index) => 320 + index * 4)
  const HEIGHTS = Array.from({ length: (1440 - 320) / 4 + 1 }, (_, index) => 320 + index * 4)

  /**
   * The property over the box `pack.ts` is actually handed, at full strength and with nothing
   * exempted: **a bigger field never draws a smaller card.**
   *
   * This is the half of the promise this module owns, and it is where the defect lived. The
   * viewport sweeps below are the same property one layer out, and they can only be as good as the
   * scene's own allocation.
   */
  it('never draws a smaller card in a bigger field', () => {
    const found: string[] = []
    for (const ladder of [
      { rows: 'split', cardTier: 'designed' },
      { rows: 'split', cardTier: 'compact' },
      { rows: 'split', cardTier: 'chip' },
      { rows: 'merged', cardTier: 'compact' },
    ] as const satisfies readonly FieldOptions[]) {
      for (const permanents of BOARDS) {
        const board = boardOf(permanents)
        for (const height of [40, 60, 100, 130, 182, 229, 300, 370, 499, 560, 800]) {
          let previous: { field: Box; tile: PackedRow } | undefined
          for (const width of WIDTHS) {
            const field = { width, height }
            const tile = tileIn(field, board, ladder)
            if (!tile) continue
            if (previous && compare(tile, previous.tile) < 0) {
              found.push(
                `${permanents} permanents, ${ladder.rows}/${ladder.cardTier}: ` +
                  `${previous.field.width}×${previous.field.height} drew ${say(previous.tile)}, ` +
                  `${width}×${height} draws ${say(tile)}`,
              )
            }
            previous = { field, tile }
          }
        }
        for (const width of [320, 390, 640, 1000, 1440, 3000]) {
          let previous: { field: Box; tile: PackedRow } | undefined
          for (let height = 20; height <= 900; height += 2) {
            const field = { width, height }
            const tile = tileIn(field, board, ladder)
            if (!tile) continue
            if (previous && compare(tile, previous.tile) < 0) {
              found.push(
                `${permanents} permanents, ${ladder.rows}/${ladder.cardTier}: ` +
                  `${previous.field.width}×${previous.field.height} drew ${say(previous.tile)}, ` +
                  `${width}×${height} draws ${say(tile)}`,
              )
            }
            previous = { field, tile }
          }
        }
      }
    }
    expect(found.slice(0, 8), `${found.length} inversions in the field`).toEqual([])
  })

  /**
   * The same property from the viewport, which is where a player meets it.
   *
   * **Nothing is exempted.** It used to skip any step at which `scene.ts` handed back a field that
   * had *shrunk* — the match line growing 8px at 640px of height, the Short band's peek strip
   * ending, the stack rail appearing at the Square boundary — because there is nothing the packing
   * can do with a box that got smaller. That exemption was the tell, and it is gone: the scene is
   * monotone by construction now (`scene.test.ts`), so the whole pipeline is, and a card that
   * shrinks on a bigger screen is a defect wherever it was introduced.
   */
  const sweep = (
    viewports: (step: number) => { width: number; height: number },
    steps: readonly number[],
  ): string[] => {
    const found: string[] = []
    for (const permanents of BOARDS) {
      const board = boardOf(permanents)
      let previous: { at: string; drawn: { tile: PackedRow; field: Box } } | undefined
      for (const step of steps) {
        const viewport = viewports(step)
        const drawn = tileAt(viewport, board)
        if (drawn === undefined) continue
        const at = `${viewport.width}×${viewport.height}`
        if (previous && compare(drawn.tile, previous.drawn.tile) < 0) {
          found.push(
            `${permanents} permanents: ${previous.at} drew ${say(previous.drawn.tile)}, ` +
              `${at} draws ${say(drawn.tile)}`,
          )
        }
        previous = { at, drawn }
      }
    }
    return found
  }

  it('never draws a smaller card on a wider viewport', () => {
    for (const height of [360, 400, 480, 600, 720, 844, 900, 1080, 1200, 1440]) {
      const found = sweep((width) => ({ width, height }), WIDTHS)
      expect(
        found.slice(0, 8),
        `${found.length} inversions across the width at ${height}px tall`,
      ).toEqual([])
    }
  })

  it('never draws a smaller card on a taller viewport', () => {
    for (const width of [390, 640, 844, 1000, 1280, 1440, 1920, 2560, 3440]) {
      const found = sweep((height) => ({ width, height }), HEIGHTS)
      expect(
        found.slice(0, 8),
        `${found.length} inversions down the height at ${width}px wide`,
      ).toEqual([])
    }
  })

  /** The sizes the inversion was reported at, named so the regression cannot come back quietly. */
  it('draws 1920×1080 at least as large as 1440×900', () => {
    for (const permanents of BOARDS) {
      const board = boardOf(permanents)
      const small = tileAt({ width: 1440, height: 900 }, board)!.tile
      for (const viewport of [
        { width: 1920, height: 1080 },
        { width: 2560, height: 1440 },
        { width: 3440, height: 1440 },
      ]) {
        const big = tileAt(viewport, board)!.tile
        expect(
          compare(big, small),
          `${permanents} permanents: 1440×900 drew ${say(small)}, ` +
            `${viewport.width}×${viewport.height} draws ${say(big)}`,
        ).toBeGreaterThanOrEqual(0)
      }
    }
  })
})
