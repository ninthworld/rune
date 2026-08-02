import { describe, expect, it } from 'vitest'

import {
  scene,
  type Rect,
  type RegionName,
  type Scene,
  type SceneCounts,
  type Viewport,
} from './scene'

/** A table mid-game: something on the stack, and nothing being asked. */
const PLAYING: SceneCounts = { stackDepth: 1 }

/**
 * The counts the scene has no field for any more.
 *
 * §5's rule is that a region's height and position are a function of the viewport alone, so a
 * permanent count and a hand size have nowhere to go: they are absorbed by the cards inside the
 * region, never by the region. Removing them from `SceneCounts` is the strongest form of that —
 * the old behaviour is not merely absent, it is unrepresentable.
 *
 * They are still written out and still passed, as excess properties `scene` ignores, so that
 * reintroducing them as geometry inputs fails here instead of quietly restoring a table that
 * rearranges itself every time a creature dies.
 */
type Ignored = Record<'yours' | 'theirs' | 'handSize', number>

/** The four board states §5 requires to be one layout. */
const BOARDS = {
  'empty on both sides': { yours: 0, theirs: 0 },
  'only yours': { yours: 12, theirs: 0 },
  'only theirs': { yours: 0, theirs: 12 },
  'both, unevenly': { yours: 12, theirs: 9 },
} satisfies Record<string, Partial<Ignored>>

const at = (viewport: Viewport, ignored: Partial<Ignored>, counts: Partial<SceneCounts> = {}) =>
  scene(viewport, { ...PLAYING, ...counts, ...ignored })

/**
 * The sizes every rule here is checked against.
 *
 * 640×360 is 1280×720 at 200% zoom, which is the whole reason the small end is taken seriously:
 * a browser at 200% does not scale the page, it halves the viewport (§1), so a desktop with the
 * text turned up and a phone in landscape are the same problem arriving twice.
 */
const VIEWPORTS: Record<string, Viewport> = {
  '3440×1440': { width: 3440, height: 1440 },
  '1920×1080': { width: 1920, height: 1080 },
  '1440×900': { width: 1440, height: 900 },
  '1280×720': { width: 1280, height: 720 },
  '1000×1000': { width: 1000, height: 1000 },
  '640×360 (1280×720 at 200%)': { width: 640, height: 360 },
  '390×844 (phone portrait)': { width: 390, height: 844 },
  '844×390 (phone landscape)': { width: 844, height: 390 },
  '320×480 (the floor)': { width: 320, height: 480 },
}

/** Everything §2 puts in tier 1. `side` is the one region allowed to be absent (§3, step 8). */
const TIER_ONE: readonly RegionName[] = [
  'header',
  'turn',
  'stack',
  'opponentSeat',
  'opponentField',
  'yourField',
  'yourSeat',
  'dock',
  'hand',
]

const drawn = (rect: Rect): boolean => rect.width > 0 && rect.height > 0

const overlapping = (a: Rect, b: Rect): boolean =>
  a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height

describe('which arrangement a viewport is in', () => {
  it('puts each of the sizes the design is checked against in the band §4 names', () => {
    const bands = Object.fromEntries(
      Object.entries(VIEWPORTS).map(([name, viewport]) => [name, scene(viewport, PLAYING).band]),
    )

    expect(bands).toEqual({
      '3440×1440': 'ultrawide',
      '1920×1080': 'wide',
      '1440×900': 'wide',
      '1280×720': 'wide',
      '1000×1000': 'square',
      '640×360 (1280×720 at 200%)': 'short',
      '390×844 (phone portrait)': 'tall',
      '844×390 (phone landscape)': 'short',
      '320×480 (the floor)': 'tall',
    })
  })

  it('says nothing can be arranged below the floor §1 states, and nothing above it', () => {
    // The floor is measured on the edges rather than on width and height: a viewport does not
    // stop being too small by being turned sideways.
    expect(scene({ width: 300, height: 400 }, PLAYING).band).toBe('unsupported')
    expect(scene({ width: 319, height: 480 }, PLAYING).band).toBe('unsupported')
    expect(scene({ width: 480, height: 319 }, PLAYING).band).toBe('unsupported')
    expect(scene({ width: 320, height: 479 }, PLAYING).band).toBe('unsupported')

    expect(scene({ width: 320, height: 480 }, PLAYING).band).toBe('tall')
    expect(scene({ width: 480, height: 320 }, PLAYING).band).toBe('short')
  })

  it('gives an unsupported viewport no regions at all', () => {
    // There is no arrangement to state, so nothing is stated. The caller draws §1's notice over
    // the whole viewport rather than attempting a board inside a box this returned.
    const { regions } = scene({ width: 300, height: 400 }, PLAYING)

    expect(Object.values(regions).filter(drawn)).toEqual([])
  })

  it('lets height decide the Short band whatever the ratio is', () => {
    // Height is the scarce resource under 480 no matter how wide the screen is, and that is the
    // whole of what Short means.
    expect(scene({ width: 2000, height: 479 }, PLAYING).band).toBe('short')
    expect(scene({ width: 2000, height: 480 }, PLAYING).band).toBe('ultrawide')
  })

  it('takes the ratio thresholds from §4 once the Short floor is past', () => {
    const bandAt = (width: number) => scene({ width, height: 1000 }, PLAYING).band

    expect(bandAt(799)).toBe('tall')
    expect(bandAt(800)).toBe('square')
    expect(bandAt(1299)).toBe('square')
    expect(bandAt(1300)).toBe('wide')
    expect(bandAt(1999)).toBe('wide')
    expect(bandAt(2000)).toBe('ultrawide')
  })

  it('has a band for a desktop at 150% zoom, which §4 as written leaves in none', () => {
    // 853×480 is 1280×720 at 150%. §4 gives Wide as "ratio ≥ 1.3, height ≥ 640", which would
    // leave this in no band at all; the height in that row describes where the designed sizes
    // survive, and that is what the ladder answers rather than what a band asserts.
    const zoomed = scene({ width: 853, height: 480 }, PLAYING)

    expect(zoomed.band).toBe('wide')
    // And what it answers here is a card face, not a chip: the hundred pixels the hand needs to
    // get off its peek strip come out of the hundred pixels above §4's Short floor, so crossing
    // that floor upward buys the hand back without taking the board's cards away (§3).
    expect(zoomed.ladder.cardTier).toBe('compact')
  })
})

describe('the boxes', () => {
  for (const [name, viewport] of Object.entries(VIEWPORTS)) {
    describe(name, () => {
      const { regions } = scene(viewport, PLAYING)
      const boxes = Object.entries(regions).filter(([, rect]) => drawn(rect))

      it('draws every region tier 1 requires', () => {
        // §2 is a contract about what a player can see without a gesture, and a region with a
        // zero box is one nobody can see. Nothing here is allowed to be the size that was left.
        expect(TIER_ONE.filter((region) => !drawn(regions[region]))).toEqual([])
      })

      it('overlaps nothing with anything', () => {
        const collisions = boxes.flatMap(([a, one], index) =>
          boxes
            .slice(index + 1)
            .filter(([, other]) => overlapping(one, other))
            .map(([b]) => `${a} × ${b}`),
        )

        expect(collisions).toEqual([])
      })

      it('stays inside the viewport on both axes', () => {
        // The invariant the whole module exists for: there is no overflow to represent, so every
        // box is inside the screen and the ladder is what gave way instead.
        const outside = boxes.filter(
          ([, rect]) =>
            rect.x < 0 ||
            rect.y < 0 ||
            rect.x + rect.width > viewport.width ||
            rect.y + rect.height > viewport.height,
        )

        expect(outside).toEqual([])
      })

      it('reads opponent, board, you, hand from the top', () => {
        // §4's one invariant. Density changes with the band; the spatial metaphor never does, so
        // what a player learns on a desktop still holds on a phone.
        const order: readonly RegionName[] = [
          'header',
          'opponentSeat',
          'opponentField',
          'yourField',
          'yourSeat',
          'dock',
          'hand',
        ]
        const tops = order.map((region) => regions[region].y)

        expect(tops).toEqual([...tops].sort((a, b) => a - b))
        // The hand is on the bottom edge, which is the only comfortably reachable region on a
        // phone and where every band puts it.
        expect(regions.hand.y + regions.hand.height).toBe(viewport.height)
      })
    })
  }
})

describe('a region is sized by the viewport, not by what is in it', () => {
  it('gives both battlefields the same height at every viewport, whatever is on them', () => {
    // The dividing line across the middle of the table does not move for any game event. A seat
    // that wipes the opponent's board does not watch its own permanents jump to a new size and a
    // new place, and a seat playing its first creature does not shove the other half upward.
    const uneven = Object.entries(VIEWPORTS).flatMap(([name, viewport]) =>
      Object.entries(BOARDS)
        .map(([board, ignored]) => [board, at(viewport, ignored).regions] as const)
        .filter(([, regions]) => regions.yourField.height !== regions.opponentField.height)
        .map(([board]) => `${name}, ${board}`),
    )

    expect(uneven).toEqual([])
  })

  it('answers every board state with exactly the same scene', () => {
    // The property the four expectations above are only samples of: with the viewport and the
    // mode fixed, *nothing* about the arrangement is a function of how much is on the table. This
    // is what makes content-driven layout unrepresentable here rather than merely absent.
    for (const [name, viewport] of Object.entries(VIEWPORTS)) {
      for (const asking of [false, true]) {
        const drew = Object.fromEntries(
          Object.entries(BOARDS).map(([board, ignored]) => [
            board,
            at(viewport, ignored, { asking }),
          ]),
        )
        const one = at(viewport, BOARDS['empty on both sides'], { asking })

        expect(drew, `${name}, asking=${asking}`).toEqual(
          Object.fromEntries(Object.keys(BOARDS).map((board) => [board, one])),
        )
      }
    }
  })

  it('keeps a seat with no permanents at the table', () => {
    // "My opponent has nothing" is one of the most important facts in the game, and it is read by
    // looking at an empty half of the table — not by noticing that a region has gone missing.
    const bare = at({ width: 1920, height: 1080 }, { yours: 0, theirs: 0 })

    expect(drawn(bare.regions.opponentField)).toBe(true)
    expect(drawn(bare.regions.yourField)).toBe(true)
    // The seat bar stays too. A seat with an empty board still has a life total, and life is
    // tier 1. What should not survive is the sentence "No permanents." printed inside the field,
    // which costs a card row's height — but that belongs to the surface that draws the field.
    expect(drawn(bare.regions.opponentSeat)).toBe(true)
  })

  it('keeps the hand a place when there is nothing in it', () => {
    const holding = at({ width: 1920, height: 1080 }, { handSize: 7 })
    const empty = at({ width: 1920, height: 1080 }, { handSize: 0 })

    expect(empty.regions.hand).toEqual(holding.regions.hand)
    expect(drawn(empty.regions.hand)).toBe(true)
  })

  it('costs nothing for an empty stack, and gives the width back to the board', () => {
    const resolving = scene({ width: 1280, height: 720 }, PLAYING)
    const empty = scene({ width: 1280, height: 720 }, { ...PLAYING, stackDepth: 0 })

    expect(empty.regions.stack).toEqual({ x: 0, y: 0, width: 0, height: 0 })
    expect(empty.regions.yourField.width).toBeGreaterThan(resolving.regions.yourField.width)
  })

  it('does not rearrange the screen when a spell resolves', () => {
    // The one place the stack's exception is deliberately not carried through. The rail's width
    // is given back to the board, but the side panel is decided against the width the rail
    // *would* take — otherwise resolving the last spell would promote a drawer to a column.
    const resolving = scene({ width: 1920, height: 1080 }, PLAYING)
    const empty = scene({ width: 1920, height: 1080 }, { ...PLAYING, stackDepth: 0 })

    expect(resolving.ladder.sidePanel).toBe('drawer')
    expect(empty.ladder.sidePanel).toBe('drawer')
  })

  it('gives a seven-deep stack and a one-deep stack the same box', () => {
    // The stack's exception is that it is an event: an event that is not happening takes no room.
    // What the exception does not license is depth — the box is decided by whether the stack
    // exists, and the seven objects are absorbed by the items in the rail exactly as a permanent
    // count is absorbed by the cards in a field.
    const one = scene({ width: 1920, height: 1080 }, { ...PLAYING, stackDepth: 1 })
    const seven = scene({ width: 1920, height: 1080 }, { ...PLAYING, stackDepth: 7 })

    expect(seven.regions.stack).toEqual(one.regions.stack)
    expect(seven).toEqual(one)

    // And on a phone, where the rail is a badge rather than a column.
    const phone = { width: 390, height: 844 }
    expect(scene(phone, { ...PLAYING, stackDepth: 7 }).regions.stack).toEqual(
      scene(phone, { ...PLAYING, stackDepth: 1 }).regions.stack,
    )
  })

  it('still describes the room when neither seat has a permanent', () => {
    // The flags read off the height a field actually got, and every field gets that height
    // whether or not anything is standing in it — so an empty table reports the arrangement it
    // will still be in once both seats are full.
    const bare = at({ width: 1920, height: 1080 }, { yours: 0, theirs: 0 })

    expect(bare.ladder.rows).toBe('split')
    expect(bare.ladder.cardTier).not.toBe('chip')
  })
})

describe('the ladder', () => {
  it('keeps card faces at the size the game is designed for', () => {
    // §1's Optimized class starts at 1280×720. A permanent there is still a card — chips at the
    // size the game is designed for would be the current failure mode with new arithmetic.
    const optimized = scene({ width: 1280, height: 720 }, PLAYING)

    expect(optimized.ladder).toEqual({
      rails: 'full',
      rows: 'split',
      cardTier: 'compact',
      sidePanel: 'drawer',
    })
  })

  it('reaches the designed presentation only where the height is there for it', () => {
    // §5's designed permanent is 130×182 and §6 reserves that presentation for a board with its
    // rows intact. Two rows of it per seat, plus a designed hand, needs more height than a 1080p
    // desktop has — so 1080p draws the compact card XMage's density argues for, and 1440p draws
    // the designed one.
    expect(scene({ width: 3440, height: 1440 }, PLAYING).ladder.cardTier).toBe('designed')
    expect(scene({ width: 1920, height: 1080 }, PLAYING).ladder.cardTier).toBe('compact')
  })

  it('drops to chips at the 100px row §3 puts the threshold at', () => {
    // Height, not width, decides whether a permanent is a card (§4). These two viewports differ
    // by a single pixel of height, and the pixel is the one that takes the row under 100.
    expect(scene({ width: 1280, height: 412 }, PLAYING).ladder.cardTier).toBe('compact')
    expect(scene({ width: 1280, height: 411 }, PLAYING).ladder.cardTier).toBe('chip')
  })

  it('merges the rows before it gives up the card face', () => {
    // Both are the Tall band and both keep card faces, which is what §4 promises portrait
    // phones. The taller one keeps its rows apart; the shorter one is better as one row of
    // 159px cards than as two rows of 79px chips, so it merges.
    const tall = scene({ width: 390, height: 844 }, PLAYING)
    const shorter = scene({ width: 375, height: 667 }, PLAYING)

    expect(tall.ladder.rows).toBe('split')
    expect(shorter.ladder.rows).toBe('merged')
    expect(shorter.ladder.cardTier).toBe('compact')
  })

  it('collapses the rails where the width has no room for them', () => {
    // Width alone, never the shape: a rail that arrived because the *ratio* crossed 0.8 would
    // leave again when the same screen got taller, and the board would lurch both ways (§3).
    expect(scene({ width: 1920, height: 1080 }, PLAYING).ladder.rails).toBe('full')
    // So a square viewport is the reference arrangement too — §4's own "nothing moves" — rather
    // than a third one with the turn laid out flat under the header.
    const square = scene({ width: 1000, height: 1000 }, PLAYING)
    expect(square.ladder.rails).toBe('full')
    expect(square.regions.turn.height).toBeGreaterThan(square.regions.turn.width)
    expect(square.regions.stack.height).toBeGreaterThan(square.regions.stack.width)

    // Collapsed: the turn is a row under the header and the stack the badge beside it, together
    // spanning the width — one arrangement, and the same one on a phone in either orientation.
    const phone = scene({ width: 390, height: 844 }, PLAYING)
    expect(phone.ladder.rails).toBe('collapsed')
    expect(phone.regions.turn.y).toBe(phone.regions.stack.y)
    expect(phone.regions.turn.width + phone.regions.stack.width).toBe(390)

    // In between, the stack has earned a rail and the turn has not: the rails are bought one at
    // a time, in the order §5 ranks them, and each is paid for before it is drawn.
    const between = scene({ width: 700, height: 700 }, PLAYING)
    expect(between.ladder.rails).toBe('collapsed')
    expect(between.regions.stack.height).toBeGreaterThan(between.regions.stack.width)
  })

  it('buys a rail out of the width past it, never out of the board', () => {
    // The three inversions this arithmetic exists to remove, each stated as the pair it broke on.
    const board = (width: number, height: number) =>
      scene({ width, height }, PLAYING).regions.yourField
    // The match line growing to its designed height at 640.
    expect(board(1280, 640).height).toBeGreaterThanOrEqual(board(1280, 639).height)
    // The hand leaving its peek strip at 480, which used to cost the board its card faces.
    expect(board(640, 480).height).toBeGreaterThanOrEqual(board(640, 476).height)
    // A rail arriving: the board is the same width across the step, and wider after it.
    expect(board(686, 800).width).toBe(board(685, 800).width)
    expect(board(900, 800).width).toBeGreaterThan(board(686, 800).width)
  })

  it('makes the side panel a column only where there is width nothing else wanted', () => {
    // §4's table, reproduced by one rule: the side is paid for out of what is left after the
    // board has taken its maximum width.
    expect(scene({ width: 3440, height: 1440 }, PLAYING).ladder.sidePanel).toBe('column')
    expect(scene({ width: 1920, height: 1080 }, PLAYING).ladder.sidePanel).toBe('drawer')
    expect(scene({ width: 1440, height: 900 }, PLAYING).ladder.sidePanel).toBe('drawer')

    const wide = scene({ width: 3440, height: 1440 }, PLAYING)
    expect(drawn(wide.regions.side)).toBe(true)
    // A drawer is drawn over the board rather than beside it, so it takes no room in the scene.
    expect(scene({ width: 1920, height: 1080 }, PLAYING).regions.side.width).toBe(0)
  })

  it('holds the board to a maximum width so a glance does not cross the whole screen', () => {
    const ultrawide = scene({ width: 3440, height: 1440 }, PLAYING)

    expect(ultrawide.regions.yourField.width).toBe(1440)
    // The rails stay pinned to the edges; it is the board that is bounded and centred.
    expect(ultrawide.regions.turn.x).toBe(0)
    expect(ultrawide.regions.stack.x + ultrawide.regions.stack.width).toBe(3440)
  })
})

describe('the hand and the action affordance trade the bottom band', () => {
  it('keeps the hand full height on a phone while nothing is being asked', () => {
    const resting = scene({ width: 390, height: 844 }, PLAYING)
    const asked = scene({ width: 390, height: 844 }, { ...PLAYING, asking: true })

    expect(asked.regions.hand.height).toBeLessThan(resting.regions.hand.height)
    // The dock takes what the hand freed — §2's rejection of a permanently reserved bar.
    expect(asked.regions.dock.height).toBeGreaterThan(resting.regions.dock.height)
    // Small, never invisible: the peek strip still shows enough of every card to count them.
    expect(asked.regions.hand.height).toBeGreaterThan(0)
  })

  it('makes the peek strip the resting state where height is the scarce resource', () => {
    // §4, Short: the hand is a peek strip by default and expands on gesture, because a phone in
    // landscape cannot afford a hand and a board at once.
    const landscape = scene({ width: 844, height: 390 }, PLAYING)
    const asked = scene({ width: 844, height: 390 }, { ...PLAYING, asking: true })

    expect(landscape.regions.hand.height).toBe(asked.regions.hand.height)
    expect(landscape.regions.hand.height).toBeLessThan(
      scene({ width: 390, height: 844 }, PLAYING).regions.hand.height,
    )
  })

  it('leaves the bottom band alone where there is room for both', () => {
    const resting = scene({ width: 1920, height: 1080 }, PLAYING)
    const asked = scene({ width: 1920, height: 1080 }, { ...PLAYING, asking: true })

    expect(asked.regions.hand.height).toBe(resting.regions.hand.height)
  })
})

/**
 * §3, "More screen is never a worse board", asserted over the arrangement itself.
 *
 * > **For a fixed board, nothing is smaller or less complete on a larger viewport than on a
 * > smaller one.**
 *
 * `pack.ts` can only be as good as the box it is handed, so the property has to hold here first:
 * a field that shrank draws a smaller card however correct the packing is. It is checked by
 * sweeping at one-pixel resolution and comparing each scene to the one a pixel smaller, rather
 * than by listing viewports and their expected boxes — a table of expectations is exactly what
 * was green while `640×476` drew 84×117 cards and `640×480`, four pixels of extra screen, drew
 * chips.
 *
 * **Nothing is exempted.** Every region is compared, in every arrangement, with one bounded
 * allowance that is itself asserted: a region that *turns* — a row under the header becoming a
 * rail down the edge — cannot keep both of its dimensions, and is compared by area instead. The
 * last test here pins the set of regions that ever take that path, so the allowance cannot
 * quietly spread: it is the turn and only the turn, and §4.1 retires that reorientation.
 */
describe('more screen is never a worse board', () => {
  const ALL: readonly RegionName[] = [
    'header',
    'turn',
    'opponentSeat',
    'opponentField',
    'stack',
    'yourField',
    'yourSeat',
    'dock',
    'hand',
    'side',
  ]

  /** The table states the scene is asked about. Fixed while the viewport moves — that is the test. */
  const STATES: Record<string, SceneCounts> = {
    'a spell on the stack': { stackDepth: 1 },
    'an empty stack': { stackDepth: 0 },
    'a question pending': { stackDepth: 1, asking: true },
  }

  const area = (rect: Rect): number => rect.width * rect.height
  const say = (rect: Rect): string => `${rect.width}×${rect.height}`
  /** Landscape or portrait. A region that changes which one it is has turned. */
  const turned = (before: Rect, after: Rect): boolean =>
    before.width >= before.height !== after.width >= after.height

  interface Sweep {
    inversions: string[]
    turns: Set<RegionName>
  }

  /** One step of a sweep: what the larger viewport took away from the smaller one, if anything. */
  const step = (
    state: string,
    before: { at: Viewport; scene: Scene },
    after: { at: Viewport; scene: Scene },
    found: Sweep,
  ): void => {
    for (const region of ALL) {
      const was = before.scene.regions[region]
      const is = after.scene.regions[region]
      if (is.width >= was.width && is.height >= was.height) continue
      if (turned(was, is) && area(is) >= area(was)) {
        found.turns.add(region)
        continue
      }
      found.inversions.push(
        `${state}, ${region}: ${before.at.width}×${before.at.height} had ${say(was)}, ` +
          `${after.at.width}×${after.at.height} has ${say(is)}`,
      )
    }
  }

  /** Every viewport a `line` walks through, each compared to the one before it. */
  const sweep = (line: (at: number) => Viewport, from: number, to: number): Sweep => {
    const found: Sweep = { inversions: [], turns: new Set() }
    for (const [state, counts] of Object.entries(STATES)) {
      let previous: { at: Viewport; scene: Scene } | undefined
      for (let at = from; at <= to; at += 1) {
        const viewport = line(at)
        const current = { at: viewport, scene: scene(viewport, counts) }
        if (previous) step(state, previous, current, found)
        previous = current
      }
    }
    return found
  }

  const report = (found: Sweep, where: string): void => {
    expect(found.inversions.slice(0, 6), `${found.inversions.length} inversions ${where}`).toEqual(
      [],
    )
    // The allowance, pinned: only the turn ever changes which way round it is drawn.
    expect([...found.turns].sort()).toEqual(found.turns.size === 0 ? [] : ['turn'])
  }

  /** §1's supported range: the floor at 320, and past every desktop this is meant to be played on. */
  const WIDTH = { from: 320, to: 3440 }
  const HEIGHT = { from: 320, to: 1440 }

  /** Heights the width sweeps are run at: the band floors, the thresholds, and ordinary screens. */
  const HEIGHTS = [320, 360, 390, 400, 479, 480, 481, 512, 600, 639, 640, 667, 720, 844, 1080, 1440]
  /** Widths the height sweeps are run at, chosen the same way. */
  const WIDTHS = [320, 375, 390, 480, 582, 640, 686, 800, 844, 1000, 1280, 1440, 1920, 2560, 3440]

  it('never hands back a smaller region on a wider viewport', () => {
    for (const height of HEIGHTS) {
      report(
        sweep((width) => ({ width, height }), WIDTH.from, WIDTH.to),
        `across the width at ${height}px tall`,
      )
    }
  })

  it('never hands back a smaller region on a taller viewport', () => {
    for (const width of WIDTHS) {
      report(
        sweep((height) => ({ width, height }), HEIGHT.from, HEIGHT.to),
        `down the height at ${width}px wide`,
      )
    }
  })

  /**
   * The same property over the plane rather than along lines through it, coarsely.
   *
   * Two sweeps at one pixel through a grid of the other dimension cannot see a step that needs
   * both dimensions to move; this compares every point to its neighbour in each direction, which
   * can — at a resolution the lines above then check exhaustively where it matters.
   */
  it('never hands back a smaller region anywhere in the supported range', () => {
    const found: Sweep = { inversions: [], turns: new Set() }
    for (let width = WIDTH.from; width <= WIDTH.to; width += 16) {
      for (let height = HEIGHT.from; height <= HEIGHT.to; height += 16) {
        for (const [state, counts] of Object.entries(STATES)) {
          const at = { width, height }
          const here = { at, scene: scene(at, counts) }
          for (const next of [
            { width: width + 16, height },
            { width, height: height + 16 },
          ]) {
            if (next.width > WIDTH.to || next.height > HEIGHT.to) continue
            step(state, here, { at: next, scene: scene(next, counts) }, found)
          }
        }
      }
    }
    report(found, 'over the supported range')
  })
})

describe('what the scene cannot say', () => {
  it('offers no way to express overflow or scrolling', () => {
    // Not an omission — the contract. §3's rule is that no region of the board ever scrolls, and
    // a type with no field for it is how that stops being a thing anyone can reach for.
    const { regions, ladder } = scene({ width: 320, height: 480 }, PLAYING)

    expect(Object.keys(ladder).sort()).toEqual(['cardTier', 'rails', 'rows', 'sidePanel'])
    expect(Object.keys(regions).sort()).toEqual([
      'dock',
      'hand',
      'header',
      'opponentField',
      'opponentSeat',
      'side',
      'stack',
      'turn',
      'yourField',
      'yourSeat',
    ])
  })

  it('answers a fractional viewport with whole pixels inside it', () => {
    // What a zoomed browser reports. Floored rather than rounded, so the union of the boxes is
    // inside the screen at every zoom level rather than one subpixel past it.
    const { regions } = scene({ width: 1279.5, height: 719.5 }, PLAYING)

    expect(regions.header.width).toBe(1279)
    expect(regions.hand.y + regions.hand.height).toBe(719)
  })
})
