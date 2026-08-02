import { describe, expect, it } from 'vitest'

import {
  fieldSlots,
  packField,
  packRow,
  type Box,
  type FieldOptions,
  type PackedRow,
  type PackOptions,
  type Point,
} from './pack'
import { CARD_MIN_HEIGHT } from './fit'
import { scene } from './scene'

/** §5's numbers, restated here so a change to one of them fails a test rather than passing. */
const CARD_FLOOR = { width: 72, height: 100 }
const CARD_DESIGNED = { width: 130, height: 182 }
const CHIP = { width: 96, height: 30 }
const RATIO = 63 / 88
/**
 * Where a tile stops being a card, imported rather than restated.
 *
 * The one number here that is *derived* — `fit.ts` takes it from §2's 9px type floor rather than
 * from §5's 100px row (`fit.test.ts` pins the derivation). Restating it would pin a value the
 * spec deliberately does not give, and the tests below want the relationship anyway.
 */
const CHIP_BELOW = CARD_MIN_HEIGHT

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
    expect(packed.width).toBe(Math.round(114 * RATIO))
    expect(Math.abs(packed.width / packed.height - RATIO)).toBeLessThan(0.02)
  })

  /**
   * §5's floor, downward: **too short a row draws a smaller card, and it stays a card.**
   *
   * The spec's own examples — a 90px row draws a 64×90 card, an 80px row a 57×80 one — with
   * everything on them set smaller rather than dropped. Nothing is given up to hold 72×100,
   * because giving something up is what §3 exists to forbid; what happens instead is that the
   * tile says it went under the minimum, and a person decides whether that is too far.
   */
  it('scales the card under the minimum rather than clamping at it', () => {
    for (const [height, width] of [
      [90, 64],
      [80, 57],
      [66, 47],
    ]) {
      const packed = packRow({ width: 1400, height: height! }, 4)
      expect(packed.tier, `${height!}px row`).not.toBe('chip')
      expect(packed.height).toBe(height)
      expect(packed.width).toBe(width)
      expect(packed.belowFloor).toBe(true)
    }
    // And says nothing where there was nothing to report.
    expect(packRow({ width: 1400, height: 100 }, 4).belowFloor).toBe(false)
    expect(packRow({ width: 600, height: 40 }, 4).belowFloor).toBe(false)
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

  /**
   * And the pitch it overlaps at is §5's, to the pixel the rounding allows — over the *turned*
   * footprint, since that is what a tile occupies (§6) and the last one still has to be able to
   * turn without leaving the row.
   */
  it('fans at the pitch §5 states', () => {
    const packed = packRow({ width: 600, height: 120 }, 20)
    expect(packed.lines).toBe(1)
    expect(packed.pitch).toBeCloseTo((600 - packed.footprint) / 19, 5)
  })

  /**
   * The one rule that can be evaluated rather than judged: **a permanent is a card while its row
   * can still set a name at §2's 9px floor**, and a landscape chip below that. The threshold is
   * `fit.ts`'s and is derived from the type floor — it is emphatically *not* the 100px row, which
   * is what made the split unaffordable and is now a review threshold instead.
   */
  it('draws a card while a name can be set, and a chip below that', () => {
    expect(packRow({ width: 600, height: CHIP_BELOW - 1 }, 4).tier).toBe('chip')
    expect(packRow({ width: 600, height: CHIP_BELOW - 1 }, 4).height).toBe(CHIP.height)
    expect(packRow({ width: 600, height: CHIP_BELOW - 1 }, 4).width).toBe(CHIP.width)
    expect(packRow({ width: 600, height: CHIP_BELOW }, 4).tier).not.toBe('chip')
    // The threshold is well under the 100px row it used to be, which is what makes three rows in
    // a field budgeted for two a board of cards rather than a board of chips.
    expect(CHIP_BELOW).toBeLessThan(CARD_FLOOR.height)
    expect(packRow({ width: 600, height: 99 }, 4).tier).not.toBe('chip')
    expect(packRow({ width: 600, height: 100 }, 4).width).toBe(CARD_FLOOR.width)
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
    // Centred in the height it did not need, and at the start of the row — inside its own turned
    // footprint, which is why `x` is the half of that footprint the card does not fill rather
    // than zero. The server's order still reads from the same edge whether one permanent is on
    // the board or twenty.
    const one = packRow(DESK, 1)
    expect(one.positions).toEqual([{ x: Math.round((one.footprint - one.width) / 2), y: 23 }])
    expect(packRow(DESK, 1).overlapped).toBe(false)
    expect(packRow(DESK, 0).positions).toEqual([])
    expect(packRow({ width: 0, height: 0 }, 4).positions).toHaveLength(4)
  })
})

/**
 * §6's turn, as the room it needs.
 *
 * **Tapped is a quarter turn**, so a permanent occupies its own *height* along the row — and the
 * row charges that whether or not anything is currently tapped, because a footprint that appeared
 * when a creature attacked would slide every other card on the board, which §5 forbids. Nothing in
 * this module has ever been told which permanents are tapped and nothing here starts: the
 * reservation is unconditional, which is the whole reason the board holds still.
 */
describe('a row reserves the footprint a turn needs', () => {
  /** Where a tile would be if it turned: the same centre, the long side across the row. */
  const turned = (packed: PackedRow, at: Point) => ({
    left: at.x + packed.width / 2 - packed.height / 2,
    right: at.x + packed.width / 2 + packed.height / 2,
  })

  it('charges every tile its turned width, tapped or not', () => {
    for (const height of [100, 114, 160, 182, 229, 400]) {
      for (const count of COUNTS) {
        const packed = packRow({ width: 1400, height }, count)
        expect(packed.footprint, `${count} at ${height}px`).toBeGreaterThanOrEqual(packed.height)
      }
    }
  })

  /** A chip is already lying down, so there is no turn to reserve for and no room to spend. */
  it('charges a chip nothing, because a chip does not turn', () => {
    for (const count of COUNTS) {
      const packed = packRow({ width: 500, height: 60 }, count, { chip: true })
      expect(packed.footprint).toBe(packed.width)
    }
  })

  /**
   * The reservation, asserted the way a player meets it: **a permanent that taps stays inside its
   * row.** At every count and every box the supported range produces — because a turned card
   * clipped by the field's own edge is the landscape footprint coming back as a defect.
   */
  it('keeps a turned tile inside the row, at every count and every size', () => {
    const found: string[] = []
    for (const width of [320, 500, 640, 1000, 1400, 3000]) {
      for (const height of [100, 114, 160, 229, 400]) {
        for (const count of COUNTS) {
          const packed = packRow({ width, height }, count)
          for (const at of packed.positions) {
            const box = turned(packed, at)
            if (box.left < -1 || box.right > width + 1) {
              found.push(`${count} at ${width}×${height}: ${box.left}…${box.right}`)
            }
          }
        }
      }
    }
    expect(found.slice(0, 8), `${found.length} turned tiles left the row`).toEqual([])
  })

  /**
   * And it does not reach its neighbour, wherever the row is not already a fan.
   *
   * An overlapped row is exempt because overlapping is what it *is* — §3's step 4 has the tiles
   * covering one another by design, and a turn there covers no more than the fan already does.
   */
  it('leaves a turned tile clear of its neighbours while the row is not fanned', () => {
    const found: string[] = []
    for (const width of [320, 640, 1400, 3000]) {
      for (const height of [100, 114, 182, 229, 400]) {
        for (const count of COUNTS) {
          const packed = packRow({ width, height }, count)
          if (packed.overlapped) continue
          const perLine = Math.ceil(count / packed.lines)
          for (let index = 1; index < count; index++) {
            if (index % perLine === 0) continue
            const before = turned(packed, packed.positions[index - 1]!)
            const after = turned(packed, packed.positions[index]!)
            if (after.left < before.right - 1) {
              found.push(`${count} at ${width}×${height}: ${before.right} over ${after.left}`)
            }
          }
        }
      }
    }
    expect(found.slice(0, 8), `${found.length} turned tiles hit a neighbour`).toEqual([])
  })

  /**
   * What it costs, said out loud: **height**.
   *
   * A crowded row draws a shorter card than the same room would have drawn upright, because the
   * room each tile divides up is its height rather than its width. That is the trade §6 makes with
   * its eyes open — a tapped permanent is one you are not currently reading — and it is asserted
   * rather than left implicit, so removing the reservation shows up here as well as in the
   * geometry above.
   */
  it('spends height on the turn where the row is crowded, and nothing where it is not', () => {
    // Uncrowded: the tile is the one the row's height affords, exactly as it was.
    expect(packRow({ width: 1400, height: 182 }, 4).height).toBe(CARD_DESIGNED.height)
    // Crowded: twelve in a 1400px row, and the footprint is what the twelve divide.
    const crowded = packRow({ width: 1400, height: 229 }, 12)
    expect(crowded.footprint).toBeLessThan(CARD_DESIGNED.height)
    expect(crowded.height).toBe(crowded.footprint)
    expect(crowded.width).toBe(Math.round(crowded.height * RATIO))
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
  /** The table, said as how many groups each half drew. Counts are not among the inputs. */
  const AT = (height: number, groups: number[], tier: FieldOptions['cardTier'] = 'compact') =>
    fieldSlots({ width: 1400, height }, groups, { rows: 'split', cardTier: tier })
  /** `Battlefield.tsx`'s collapse, as counts: the groups furthest from the middle merge first. */
  const collapse = (counts: readonly number[], rows: number): readonly number[] =>
    counts.length <= rows
      ? counts
      : [...counts.slice(0, rows - 1), counts.slice(rows - 1).reduce((all, one) => all + one, 0)]

  /**
   * The rule, and there is only one: **a field draws one row per group the board has** (§5, "The
   * row count is the board's, never the card's"). Not one per group it can draw at 100px, and not
   * the count whose worst row draws the biggest tile — that objective always merges, because one
   * row of `N` cards is never smaller than three rows of the same `N`.
   */
  it('draws one row per group the board has', () => {
    expect(AT(600, [3, 2])).toBe(3)
    expect(AT(600, [2, 2])).toBe(2)
    expect(AT(600, [1, 1])).toBe(1)
    // The three sizes the maintainer named, at the field the scene gives each of them. None of
    // them has the room for three 100px rows; all three keep their rows.
    expect(AT(200, [3, 3])).toBe(3) // 1280×720
    expect(AT(319, [3, 3])).toBe(3) // 1920×1080
    expect(AT(499, [3, 3])).toBe(3) // 2560×1440
  })

  /** And the count in a row is not among its inputs: a crowded board is not a merged one. */
  it('answers the same for a crowded board as for an empty one', () => {
    for (const height of [120, 200, 229, 319, 400, 800]) {
      expect(fieldSlots({ width: 390, height }, [3, 3], SPLIT)).toBe(AT(height, [3, 3]))
      expect(fieldSlots({ width: 3000, height }, [3, 3], SPLIT)).toBe(AT(height, [3, 3]))
    }
  })

  /**
   * What it costs, said out loud: **the cards get smaller, and they stay cards.**
   *
   * A field budgeted for two rows of the 100px minimum, asked for three, draws three rows of 66px
   * cards — not one row of 182px ones, and not three rows of chips. Each of them reports that it
   * went under the minimum, because §3 makes that a review threshold and not a licence to drop.
   */
  it('pays for the split in card size, not in rows or in faces', () => {
    const field = { width: 1400, height: 200 }
    const plan = packField(field, [4, 4, 4], { slots: AT(200, [3, 3]), cardTier: 'compact' })
    expect(plan.rows).toHaveLength(3)
    for (const row of plan.rows) {
      expect(row.pack.tier).not.toBe('chip')
      expect(row.pack.height).toBeLessThan(CARD_FLOOR.height)
      expect(row.pack.belowFloor).toBe(true)
      expect(row.pack.width).toBe(Math.round(row.pack.height * RATIO))
    }
    // And the same field merged would have drawn the designed card, which is exactly the trade
    // §3 asks for and the old objective refused to make.
    const merged = packField(field, [12], { slots: 1, cardTier: 'compact' })
    expect(merged.rows[0]!.pack.height).toBeGreaterThan(plan.rows[0]!.pack.height)
  })

  /**
   * The one thing that still takes a row away: **a row that cannot draw a tile at all.**
   *
   * Stated in terms of the tier's own minimum — the chip's 30px, since a row too short for a card
   * already draws a chip — and it is the bottom of the ladder rather than a size optimisation. An
   * 89px field cannot give three rows 30px each; a 90px one can.
   */
  it('falls below the group count only where a row cannot draw a tile at all', () => {
    expect(AT(90, [3, 3])).toBe(3)
    expect(AT(89, [3, 3])).toBe(2)
    expect(AT(59, [3, 3])).toBe(1)
    expect(AT(90, [2, 2], 'chip')).toBe(2)
    expect(AT(50, [2, 2], 'chip')).toBe(1)
    // Every row it did keep can draw something, and every row the count it refused would have
    // had cannot. That is the condition, asserted rather than the number it comes out at.
    for (const height of [40, 59, 60, 89, 90, 120, 200]) {
      const rows = AT(height, [3, 3])
      const plan = packField({ width: 1400, height }, collapse([2, 2, 2], rows), {
        slots: rows,
        cardTier: 'compact',
      })
      expect(plan.rows, `${height}px`).toHaveLength(rows)
      for (const row of plan.rows) expect(row.pack.height).toBeGreaterThanOrEqual(CHIP.height)
    }
  })

  /**
   * And the viewport where that actually happens, named.
   *
   * On a supported screen it is never this fallback that merges a board: `scene.ts` applies §3's
   * step 6 first, and it does so while the field still has room for one tall row. 320×480 — the
   * smallest screen the client supports — is that viewport, and the merge is the *scene's*.
   */
  it('merges at 320×480, and by the scene’s answer rather than its own', () => {
    const { regions, ladder } = scene({ width: 320, height: 480 }, { stackDepth: 1 })
    expect(ladder.rows).toBe('merged')
    expect(fieldSlots(regions.yourField, [3, 3], ladder)).toBe(1)
    // The same field with the ladder still split keeps all three rows: nothing about the room
    // merged it, only the step the scene had already taken.
    expect(fieldSlots(regions.yourField, [3, 3], SPLIT)).toBe(3)
  })

  /** Answered for the table, so the half with the most groups decides for both of them. */
  it('answers once for every half', () => {
    expect(AT(229, [3, 1])).toBe(AT(229, [1, 3]))
    expect(AT(400, [3, 1])).toBe(AT(400, [1, 3]))
    expect(AT(229, [3, 1])).toBe(3)
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

  /**
   * And no tile is ever below the minimum of the tier it is drawing — which for a card is where
   * its name stops fitting, not §5's 72×100.
   *
   * A card under 72×100 is drawn and *reported*: `belowFloor` is the review threshold §3 asks for,
   * and it is asserted here as the thing it is — a fact about the tile that a caller can surface,
   * true exactly when the tile went under the minimum.
   */
  it('never draws a tile under the minimum of its own tier, and says when it went under 72×100', () => {
    for (const viewport of VIEWPORTS) {
      const { regions, ladder } = scene(viewport, { stackDepth: 1 })
      for (const count of COUNTS) {
        const slots = fieldSlots(regions.yourField, [3, 2], ladder)
        const plan = packField(regions.yourField, [count], { slots, cardTier: ladder.cardTier })
        const packed = plan.rows[0]?.pack
        if (!packed) continue
        const at = `${viewport.width}×${viewport.height}, ${count}`
        if (packed.tier === 'chip') {
          expect(packed.width, at).toBe(CHIP.width)
          expect(packed.belowFloor, at).toBe(false)
        } else {
          expect(packed.height, at).toBeGreaterThanOrEqual(CHIP_BELOW)
          expect(packed.belowFloor, at).toBe(packed.height < CARD_FLOOR.height)
        }
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

  /** What one board came out as: how many rows it was drawn in, and the worst tile among them. */
  interface Drawn {
    rows: number
    tile: PackedRow
  }

  /**
   * How two boards compare, negative when the first is the worse one.
   *
   * **Rows lead**, because that is what §3 says a board is read by: the scan by category is worth
   * more than any particular card size, and three rows of smaller cards is the trade the ladder
   * asks for rather than a regression. Sweeping the card size alone would call the moment a field
   * earns its split — one 199px row of 142px cards becoming three 66px rows of 47px ones — the
   * worst inversion in the file, when it is the fix.
   *
   * Then the tier, because a chip is not a narrow card: going from a chip to a card is an
   * improvement even where the card is narrower than the 96px chip, and going the other way is
   * the ladder's step 5 and a loss however wide the chip is. Then the width, which is the size
   * question the sweep was written for in the first place.
   */
  const rank = (packed: PackedRow): number => (packed.tier === 'chip' ? 0 : 1)
  const compare = (a: Drawn, b: Drawn): number =>
    a.rows - b.rows || rank(a.tile) - rank(b.tile) || a.tile.width - b.tile.width
  /** The worse of two tiles, tier before size — used to find the worst row inside one board. */
  const poorer = (a: PackedRow, b: PackedRow): boolean =>
    rank(a) - rank(b) < 0 || (rank(a) === rank(b) && a.width < b.width)
  const say = (drawn: Drawn): string =>
    `${drawn.rows} row${drawn.rows === 1 ? '' : 's'} of ${drawn.tile.width}×${drawn.tile.height} ` +
    `${drawn.tile.tier}${drawn.tile.lines > 1 ? ` ×${drawn.tile.lines}` : ''}`

  /**
   * What one field draws for one board — the row count, and the *worst* row of it, since a board
   * is only as readable as the smallest card on it. `undefined` where the viewport is below §1's
   * floor and there is no board to draw at all.
   */
  const tileIn = (
    field: Box,
    board: readonly number[],
    ladder: FieldOptions,
  ): Drawn | undefined => {
    if (field.width <= 0 || field.height <= 0) return undefined
    const slots = fieldSlots(field, [board.length, board.length], ladder)
    // `Battlefield.tsx`'s collapse: the rows furthest from the dividing line merge first.
    const counts =
      board.length <= slots
        ? board
        : [
            ...board.slice(0, slots - 1),
            board.slice(slots - 1).reduce((sum, count) => sum + count, 0),
          ]
    const plan = packField(field, counts, { slots, cardTier: ladder.cardTier })
    const tile = plan.rows.reduce<PackedRow | undefined>(
      (worst, row) => (worst === undefined || poorer(row.pack, worst) ? row.pack : worst),
      undefined,
    )
    return tile && { rows: plan.rows.length, tile }
  }

  const tileAt = (
    viewport: { width: number; height: number },
    board: readonly number[],
  ): { tile: Drawn; field: Box } | undefined => {
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
          let previous: { field: Box; tile: Drawn } | undefined
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
          let previous: { field: Box; tile: Drawn } | undefined
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
      let previous: { at: string; drawn: { tile: Drawn; field: Box } } | undefined
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

  /**
   * The half of "less complete" the size sweeps cannot see: **the number of rows never falls.**
   *
   * A board read as creatures, other permanents and lands does not become a board read as one
   * list because the screen got bigger, and it is the row count that says so — the tile sweeps
   * above rank a deeper split as the better board, so a merge that came with a bigger card would
   * pass them silently. It is swept down the height rather than named at viewports because the
   * merge that this issue is about happened at every desktop size below ultrawide and every table
   * of expected values had been green through all of them.
   */
  it('never draws fewer rows on a taller viewport', () => {
    const found: string[] = []
    for (const width of [390, 640, 844, 1000, 1280, 1440, 1920, 2560, 3440]) {
      for (const permanents of BOARDS) {
        const board = boardOf(permanents)
        let previous: { at: string; drawn: Drawn } | undefined
        for (const height of HEIGHTS) {
          const drawn = tileAt({ width, height }, board)
          if (drawn === undefined) continue
          const at = `${width}×${height}`
          if (previous && drawn.tile.rows < previous.drawn.rows) {
            found.push(
              `${permanents} permanents in ${board.length} groups: ` +
                `${previous.at} drew ${say(previous.drawn)}, ${at} draws ${say(drawn.tile)}`,
            )
          }
          previous = { at, drawn: drawn.tile }
        }
      }
    }
    expect(found.slice(0, 8), `${found.length} rows lost on a taller viewport`).toEqual([])
  })

  /** And never on a wider one either — the row count is the board's, so width cannot move it. */
  it('never draws fewer rows on a wider viewport', () => {
    const found: string[] = []
    for (const height of [360, 480, 720, 900, 1080, 1440]) {
      for (const permanents of BOARDS) {
        const board = boardOf(permanents)
        let previous: { at: string; drawn: Drawn } | undefined
        for (const width of WIDTHS) {
          const drawn = tileAt({ width, height }, board)
          if (drawn === undefined) continue
          const at = `${width}×${height}`
          if (previous && drawn.tile.rows < previous.drawn.rows) {
            found.push(
              `${permanents} permanents: ${previous.at} drew ${say(previous.drawn)}, ` +
                `${at} draws ${say(drawn.tile)}`,
            )
          }
          previous = { at, drawn: drawn.tile }
        }
      }
    }
    expect(found.slice(0, 8), `${found.length} rows lost on a wider viewport`).toEqual([])
  })

  /**
   * The three sizes the maintainer named, with a realistic board on **both** halves.
   *
   * "The creatures and lands mixing together is unacceptable." Every one of these merged before
   * this change, at every count, because a merged row of `N` cards is never smaller than three
   * rows of the same `N` — so the packer that was told to maximise the tile merged them all.
   */
  it('keeps creatures, other permanents and lands apart at 1280×720, 1920×1080 and 2560×1440', () => {
    for (const viewport of [
      { width: 1280, height: 720 },
      { width: 1920, height: 1080 },
      { width: 2560, height: 1440 },
    ]) {
      const { regions, ladder } = scene(viewport, { stackDepth: 1 })
      const at = `${viewport.width}×${viewport.height}`
      expect(ladder.rows, at).toBe('split')
      // Both halves have all three groups, which is the board a real game reaches.
      expect(fieldSlots(regions.yourField, [3, 3], ladder), at).toBe(3)
      // And a half with only two of them still draws two, in the table's own slots.
      expect(fieldSlots(regions.yourField, [3, 2], ladder), at).toBe(3)

      const plan = packField(regions.yourField, [5, 2, 5], { slots: 3, cardTier: ladder.cardTier })
      expect(plan.rows, at).toHaveLength(3)
      for (const row of plan.rows) {
        // Cards, at whatever size three rows cost — never chips, and never off the box.
        expect(row.pack.tier, at).not.toBe('chip')
        expect(row.y + row.height, at).toBeLessThanOrEqual(regions.yourField.height)
      }
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
