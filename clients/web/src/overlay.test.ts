import { describe, expect, it } from 'vitest'

import { intersect, overlayEdges, type Rect } from './overlay'
import type { Relation } from './relations'

/** The frame every case is measured inside: a 1000×600 board with its origin at the corner. */
const FRAME: Rect = { x: 0, y: 0, width: 1000, height: 600 }

const box = (x: number, y: number): Rect => ({ x, y, width: 100, height: 100 })

/** Two 100-square boxes, one directly above the other with 100px of board between them. */
const STACKED = new Map<string, Rect>([
  ['attacker', box(200, 100)],
  ['blocker', box(200, 300)],
])

describe('a relationship as a segment', () => {
  it('runs between the two borders that face each other, not between the two centres', () => {
    // A line drawn centre to centre crosses both card faces to get anywhere. The visible part
    // of it would be the gap between them either way, so it starts and ends at the edge.
    const edges = overlayEdges(
      [{ kind: 'blocking', from: 'blocker', to: 'attacker' }],
      STACKED,
      FRAME,
    )

    // Up the middle of both boxes: out of the blocker's top edge, into the attacker's bottom.
    expect(edges).toEqual([
      {
        kind: 'blocking',
        from: 'blocker',
        to: 'attacker',
        x1: 250,
        y1: 300,
        x2: 250,
        y2: 200,
        emphasis: 'plain',
      },
    ])
  })

  it('leaves through the side the other object is actually on', () => {
    // The nearer of the four sides, for any direction. A segment that always left through the
    // top would point out of the wrong face for every relationship across the table.
    const anchors = new Map<string, Rect>([
      ['aura', box(100, 100)],
      ['creature', box(700, 120)],
    ])

    const edges = overlayEdges([{ kind: 'attached', from: 'aura', to: 'creature' }], anchors, FRAME)

    // Out of the aura's right edge and into the creature's left, rather than out of either top.
    expect(edges.map((edge) => [edge.x1, edge.x2])).toEqual([[200, 700]])
  })

  it('draws no line for an edge whose other end the server never named', () => {
    // An attacker with no defender projected: the view stated the attack and declined to state
    // what it is aimed at. Pointing the arrow anywhere would be this client answering that.
    expect(overlayEdges([{ kind: 'attacking', from: 'attacker' }], STACKED, FRAME)).toEqual([])
  })

  it('draws no line to an object this screen is not drawing', () => {
    // The other end is in a pile nobody opened. The trail under the card still names it in
    // words, which is the copy of the fact that never depends on where anything is.
    const edges = overlayEdges(
      [{ kind: 'targeting', from: 'attacker', to: 'card-in-a-graveyard' }],
      STACKED,
      FRAME,
    )

    expect(edges).toEqual([])
  })

  it('draws no line to an object that has scrolled out of the board', () => {
    // A region scrolls inside its own area, so an anchored object can sit outside the frame
    // entirely. An arrow to it would leave the board and point at the chrome.
    const anchors = new Map<string, Rect>([
      ['attacker', box(200, 100)],
      ['blocker', box(200, 900)],
    ])

    expect(
      overlayEdges([{ kind: 'blocking', from: 'blocker', to: 'attacker' }], anchors, FRAME),
    ).toEqual([])
  })

  it('keeps an object the frame shows any part of', () => {
    // Half off the bottom edge is still on the board, and the half that is showing is where the
    // arrow lands.
    const anchors = new Map<string, Rect>([
      ['attacker', box(200, 100)],
      ['blocker', { x: 200, y: 550, width: 100, height: 100 }],
    ])

    expect(
      overlayEdges([{ kind: 'blocking', from: 'blocker', to: 'attacker' }], anchors, FRAME),
    ).toHaveLength(1)
  })

  it('draws no line from an object to itself', () => {
    const anchors = new Map<string, Rect>([['loop', box(200, 100)]])

    expect(overlayEdges([{ kind: 'attached', from: 'loop', to: 'loop' }], anchors, FRAME)).toEqual(
      [],
    )
  })
})

describe('how much of an object is actually showing', () => {
  it('keeps the part of a box its region has not scrolled away', () => {
    // Every region of the board scrolls inside its own area, so being laid out and being on the
    // screen are different things. What is left is where the arrow lands.
    expect(
      intersect(
        { x: 100, y: 100, width: 100, height: 100 },
        { x: 0, y: 150, width: 500, height: 500 },
      ),
    ).toEqual({ x: 100, y: 150, width: 100, height: 50 })
  })

  it('answers with nothing for a box its region scrolled away entirely', () => {
    // Not an anchor at all: an arrow to it would point confidently at a card nobody can see, and
    // the trail under the card is what still names it in words.
    expect(
      intersect(
        { x: 100, y: 700, width: 100, height: 100 },
        { x: 0, y: 0, width: 500, height: 500 },
      ),
    ).toBeUndefined()
  })

  it('answers with nothing for a box that only touches the edge', () => {
    // A zero-height sliver is not something a player can see or click.
    expect(
      intersect({ x: 0, y: 500, width: 100, height: 100 }, { x: 0, y: 0, width: 500, height: 500 }),
    ).toBeUndefined()
  })
})

describe('what the player is looking at', () => {
  const ALL: readonly Relation[] = [
    { kind: 'blocking', from: 'blocker', to: 'attacker' },
    { kind: 'targeting', from: 'spell', to: 'bystander' },
  ]
  const ANCHORS = new Map<string, Rect>([
    ...STACKED,
    ['spell', box(700, 100)],
    ['bystander', box(700, 300)],
  ])

  it('emphasises nothing while the player is looking at nothing', () => {
    expect(overlayEdges(ALL, ANCHORS, FRAME).map((edge) => edge.emphasis)).toEqual([
      'plain',
      'plain',
    ])
  })

  it('raises the edges that touch what is being looked at, from either end', () => {
    // Either end: the object a player hovers is as often the one being pointed *at* — an
    // attacker knows nothing about being blocked — as the one doing the pointing.
    const edges = overlayEdges(ALL, ANCHORS, FRAME, { traced: 'attacker' })

    expect(edges.map((edge) => [edge.from, edge.emphasis])).toEqual([
      ['spell', 'dimmed'],
      ['blocker', 'traced'],
    ])
  })

  it('paints the traced edges last so they are on top of the board', () => {
    // The only ranking in the module, and it lasts exactly as long as the pointer stays put.
    const edges = overlayEdges(ALL, ANCHORS, FRAME, { traced: 'spell' })

    expect(edges.map((edge) => edge.emphasis)).toEqual(['dimmed', 'traced'])
  })
})
