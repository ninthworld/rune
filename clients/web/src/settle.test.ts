/**
 * The settle report, against the wire's own vocabulary (issue #709).
 *
 * What is contract here rather than taste — and what each test exists to fail on if it is ever
 * "simplified" away:
 *
 * - it is a **pure reading of one view**, so a newer view replaces it wholesale and a reconnect
 *   shows nothing. That is what makes a stale report impossible rather than guarded against;
 * - it never reports on a view the player was actually asked for;
 * - it is bounded, and says how much it left out rather than quietly dropping it;
 * - the words are the **log's own**, so the band and the log cannot describe one event two ways.
 */
import { describe, expect, it } from 'vitest'

import type { GameLogEntry, GameView } from './protocol'
import { SHOWN, settleOf } from './settle'

const name = (id: string): string => (id === 'p0' ? 'You' : 'Ada')

const view = (over: Partial<GameView> = {}): GameView => ({
  you: 'p0',
  phase: 'precombat_main',
  turn: 3,
  active_player: 'p0',
  ...over,
})

/** A log entry of the shape the wire sends, at `sequence`. */
const drew = (sequence: number, count = 1): GameLogEntry => ({
  sequence,
  event: { type: 'cards_drawn', player: 'p0', count },
})

const died = (sequence: number, name: string): GameLogEntry => ({
  sequence,
  event: {
    type: 'permanent_died',
    permanent: { id: `perm_${name}`, name },
  },
})

const stepped = (sequence: number, phase: 'begin_combat'): GameLogEntry => ({
  sequence,
  event: { type: 'step_changed', phase, turn: 3, active_player: 'p0' },
})

describe('a view that describes no settle', () => {
  it('reports nothing when the server marked none', () => {
    // The ordinary case: a player being asked something missed nothing, and a band drawn here
    // would be the client inventing an event.
    expect(settleOf(view(), name)).toBeUndefined()
  })

  it('reports nothing when a log exists but no mark points into it', () => {
    // A log is not a settle. Without `auto_passed_from` the server has not said this receiver
    // was skipped over, and every entry in the window is one they were already sent.
    expect(settleOf(view({ log: [drew(1), died(2, 'Grizzly Bears')] }), name)).toBeUndefined()
  })
})

describe('what a settle did', () => {
  it('names where the game went and what happened on the way', () => {
    const settle = settleOf(
      view({
        phase: 'postcombat_main',
        auto_passed_from: 2,
        auto_passed_steps: [
          { phase: 'begin_combat', turn: 3 },
          { phase: 'declare_attackers', turn: 3 },
        ],
        log: [drew(1), died(2, 'Grizzly Bears')],
      }),
      name,
    )
    expect(settle?.path).toBe('Passed 2 steps, now at Postcombat main')
    // The draw at sequence 1 is before the mark: this receiver was sent it, so it is not news.
    expect(settle?.events.map((event) => event.text)).toEqual(['Grizzly Bears dies'])
    expect(settle?.more).toBe(0)
  })

  it('leaves the step changes to the path rather than repeating them as events', () => {
    // The path already says where the game went. A "the game moved to combat" line beside it is
    // the same fact twice, in a band whose whole budget is three lines.
    const settle = settleOf(
      view({
        auto_passed_from: 1,
        auto_passed_steps: [{ phase: 'begin_combat', turn: 3 }],
        log: [stepped(1, 'begin_combat'), died(2, 'Grizzly Bears')],
      }),
      name,
    )
    expect(settle?.events.map((event) => event.text)).toEqual(['Grizzly Bears dies'])
  })

  it('is bounded, and says how many it left out', () => {
    const log = [died(1, 'One'), died(2, 'Two'), died(3, 'Three'), died(4, 'Four'), died(5, 'Five')]
    const settle = settleOf(
      view({ auto_passed_from: 1, auto_passed_steps: [{ phase: 'end', turn: 3 }], log }),
      name,
    )
    expect(settle?.events).toHaveLength(SHOWN)
    // The **most recent**, because the board on screen is the state they produced.
    expect(settle?.events.map((event) => event.text)).toEqual([
      'Three dies',
      'Four dies',
      'Five dies',
    ])
    expect(settle?.more).toBe(2)
  })

  it('says when the settle crossed into another turn', () => {
    const settle = settleOf(
      view({
        turn: 4,
        auto_passed_from: 1,
        auto_passed_steps: [
          { phase: 'end', turn: 3 },
          { phase: 'upkeep', turn: 4 },
        ],
        log: [drew(1)],
      }),
      name,
    )
    expect(settle?.crossedTurn).toBe(true)
  })

  it('does not call a settle within one turn a crossing', () => {
    const settle = settleOf(
      view({
        auto_passed_from: 1,
        auto_passed_steps: [
          { phase: 'begin_combat', turn: 3 },
          { phase: 'end_combat', turn: 3 },
        ],
        log: [drew(1)],
      }),
      name,
    )
    expect(settle?.crossedTurn).toBe(false)
  })

  it('names a single passed step by its own name', () => {
    const settle = settleOf(
      view({
        auto_passed_from: 1,
        auto_passed_steps: [{ phase: 'upkeep', turn: 3 }],
        log: [drew(1)],
      }),
      name,
    )
    expect(settle?.path).toBe('Passed Upkeep')
  })
})

describe('the guarantee that it holds nothing', () => {
  it('is replaced wholesale by the next view', () => {
    const settled = view({
      auto_passed_from: 1,
      auto_passed_steps: [{ phase: 'upkeep', turn: 3 }],
      log: [died(1, 'Grizzly Bears')],
    })
    expect(settleOf(settled, name)).toBeDefined()

    // The next view is one this player was asked for. Nothing carries over — there is no queue
    // to interrupt and no timer to cancel, which is the whole design.
    const asked = view({ log: [died(1, 'Grizzly Bears')] })
    expect(settleOf(asked, name)).toBeUndefined()
  })

  it('shows nothing after a reconnect, because the mark is what is missing', () => {
    // A reconnect lands on the current canonical state. The log may still hold everything that
    // happened, but the server did not mark this receiver as having been skipped over — so
    // there is nothing to replay, rather than something to suppress.
    const reconnected = view({ log: [died(1, 'Grizzly Bears'), drew(2)] })
    expect(settleOf(reconnected, name)).toBeUndefined()
  })

  it("describes an event in the log's own words", () => {
    // One describer, so the band and the log cannot disagree about what happened. If the log's
    // phrasing changes, this changes with it and nothing has to be kept in step by hand.
    const settle = settleOf(
      view({
        auto_passed_from: 1,
        auto_passed_steps: [{ phase: 'upkeep', turn: 3 }],
        log: [drew(1, 2)],
      }),
      name,
    )
    const [only] = settle?.events ?? []
    expect(only?.text).toContain('2')
    expect(only?.kind).toBe('zone')
  })
})
