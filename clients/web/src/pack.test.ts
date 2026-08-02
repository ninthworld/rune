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
  const AT = (height: number, groups: number[], tier: FieldOptions['cardTier'] = 'compact') =>
    fieldSlots({ width: 1400, height }, groups, { rows: 'split', cardTier: tier })

  /** One row per group the table needs, while every one of them still holds a card face. */
  it('gives the table a row for every group the busiest half has', () => {
    expect(AT(229, [2, 2])).toBe(2)
    expect(AT(200, [2, 1])).toBe(2)
    expect(AT(400, [1, 1])).toBe(1)
    expect(AT(400, [3, 2])).toBe(3)
  })

  /**
   * And merges when they will not, which is §3's order stated outright: merging is step 5 and
   * chips are step 6, so the rows go before the faces do. The third row is the case the scene
   * cannot see, because it funds two.
   */
  it('merges when a row would fall under the card floor', () => {
    expect(AT(199, [2, 2])).toBe(1)
    expect(AT(200, [2, 2])).toBe(2)
    expect(AT(229, [3, 2])).toBe(1)
    expect(AT(299, [3, 2])).toBe(1)
    expect(AT(300, [3, 2])).toBe(3)
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
      fieldSlots({ width: 1400, height: 400 }, [3, 2], { rows: 'merged', cardTier: 'compact' }),
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
        const slots = fieldSlots(regions.yourField, [3, 2], ladder)
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
