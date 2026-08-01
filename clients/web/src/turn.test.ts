/**
 * Turn flow, against the wire's own vocabulary.
 *
 * Three things here are contract rather than taste, and each has a test that fails loudly if it
 * is ever "simplified" away:
 *
 * - a settle's path may cross a turn boundary, so only entries whose `turn` matches the view's
 *   may mark a step in it;
 * - `set_stops` replaces the whole preference, so a change is composed from what the server is
 *   currently reflecting and never from a remembered delta;
 * - the path is a path, not a set — a position reached twice appears twice.
 */
import { describe, expect, it } from 'vitest'

import type { GameView } from './protocol'
import {
  PHASES,
  matchStatus,
  nextScope,
  passedRuns,
  phaseLabel,
  presetOf,
  presetStops,
  statusLine,
  steps,
  withStop,
  type Step,
} from './turn'

const view = (over: Partial<GameView> = {}): GameView => ({
  you: 'p0',
  phase: 'precombat_main',
  turn: 3,
  active_player: 'p0',
  ...over,
})

const at =
  (phase: string) =>
  (all: readonly Step[]): Step =>
    all.find((step) => step.phase === phase)!

describe('naming a step', () => {
  it('spells out the steps this build knows', () => {
    expect(phaseLabel('precombat_main')).toBe('Precombat main')
    expect(phaseLabel('declare_blockers')).toBe('Declare blockers')
  })

  it('renders an unknown classifier as sent rather than guessing at it', () => {
    expect(phaseLabel('interstitial')).toBe('interstitial')
  })
})

describe('the strip', () => {
  it('draws every step of a turn, in turn order', () => {
    expect(steps(view()).map((step) => step.phase)).toEqual(PHASES)
    expect(PHASES[0]).toBe('untap')
    expect(PHASES.at(-1)).toBe('cleanup')
  })

  it('marks the step the server says the game is in, and only that one', () => {
    const marked = steps(view({ phase: 'declare_blockers' })).filter((step) => step.current)
    expect(marked.map((step) => step.phase)).toEqual(['declare_blockers'])
  })

  it('reads the effective stop preference the server reflected', () => {
    const strip = steps(view({ stops: ['end'], own_turn_stops: ['precombat_main'] }))
    expect(at('end')(strip).stop).toBe('always')
    expect(at('precombat_main')(strip).stop).toBe('own')
    expect(at('upkeep')(strip).stop).toBe('none')
  })

  it('lets the wider claim win if a server ever sent a step on both lists', () => {
    // The wire never carries one on both (`docs/protocol.md`), so this is about not drawing a
    // step as two things at once if it ever did.
    const strip = steps(view({ stops: ['end'], own_turn_stops: ['end'] }))
    expect(at('end')(strip).stop).toBe('always')
  })

  it('marks only the passed steps belonging to the turn on screen', () => {
    // A settle that crossed a boundary carries the previous turn's positions too. Marking those
    // in this turn's strip would claim the game skipped a step it has not reached yet.
    const strip = steps(
      view({
        turn: 3,
        auto_passed_steps: [
          { turn: 2, phase: 'end' },
          { turn: 3, phase: 'upkeep' },
        ],
      }),
    )
    expect(at('upkeep')(strip).passed).toBe(true)
    expect(at('end')(strip).passed).toBe(false)
  })

  it('marks nothing when the view stated no turn to match against', () => {
    const strip = steps(
      view({ turn: undefined, auto_passed_steps: [{ turn: 2, phase: 'upkeep' }] }),
    )
    expect(strip.some((step) => step.passed)).toBe(false)
  })
})

describe('setting a stop', () => {
  it('sends the whole preference, not the one step that changed', () => {
    const current = view({ stops: ['end'], own_turn_stops: ['precombat_main'] })
    expect(withStop(current, 'upkeep', 'always')).toEqual({
      type: 'set_stops',
      stops: ['upkeep', 'end'],
      own_turn: ['precombat_main'],
    })
  })

  it('moves a step between the two lists rather than adding it to both', () => {
    const current = view({ own_turn_stops: ['precombat_main'] })
    expect(withStop(current, 'precombat_main', 'always')).toEqual({
      type: 'set_stops',
      stops: ['precombat_main'],
    })
  })

  it('clears a stop by omitting it, and says "nowhere" with the minimal message', () => {
    // Two empty lists mean "stop nowhere", not "leave my defaults alone" — which is the only way
    // a player can clear the stops the server seeds for a human seat.
    const current = view({ own_turn_stops: ['precombat_main'] })
    expect(withStop(current, 'precombat_main', 'none')).toEqual({ type: 'set_stops' })
  })

  it('keeps turn order, so the same preference always serializes the same way', () => {
    const current = view({ stops: ['end', 'upkeep'] })
    expect(withStop(current, 'draw', 'always')).toEqual({
      type: 'set_stops',
      stops: ['upkeep', 'draw', 'end'],
    })
  })

  it('cycles a control through off, your turn, every turn', () => {
    expect(nextScope('none')).toBe('own')
    expect(nextScope('own')).toBe('always')
    expect(nextScope('always')).toBe('none')
  })
})

describe('who the game is waiting for', () => {
  const label = (id: string) => (id === 'p0' ? 'You' : 'Alice')

  it('puts a finished game ahead of everything else', () => {
    const over = view({
      result: { reason: 'concede', winner: 'p1' },
      valid_actions: [{ id: 'a1', type: 'concede', label: 'Concede' }],
    })
    expect(matchStatus(over, 'Pass')).toEqual({ kind: 'over' })
  })

  it('puts a click in flight ahead of the actions that are still offered', () => {
    const acting = view({ valid_actions: [{ id: 'a1', type: 'pass_priority', label: 'Pass' }] })
    expect(matchStatus(acting, 'Pass')).toEqual({ kind: 'sent', label: 'Pass' })
    expect(statusLine(matchStatus(acting, 'Pass'), label)).toContain('waiting for the server')
  })

  it('says it is your move exactly when the server offered you something', () => {
    const acting = view({ valid_actions: [{ id: 'a1', type: 'pass_priority', label: 'Pass' }] })
    expect(matchStatus(acting)).toEqual({ kind: 'yours' })
    expect(statusLine(matchStatus(acting), label)).toBe('Your move.')
  })

  it('names the priority holder rather than guessing what they are doing', () => {
    const waiting = view({ priority_player: 'p1', valid_actions: [] })
    expect(statusLine(matchStatus(waiting), label)).toBe('Waiting for Alice.')
  })

  it('leaves the game as the subject when no seat holds priority', () => {
    // A settle running between two broadcasts: nobody has been handed priority, and inventing
    // a seat to blame would be a rules claim.
    expect(statusLine(matchStatus(view({ valid_actions: [] })), label)).toContain(
      'moving on its own',
    )
  })
})

describe('what the settle did on your behalf', () => {
  it('groups the path into per-turn runs, in the order it acted', () => {
    expect(
      passedRuns([
        { turn: 2, phase: 'end' },
        { turn: 2, phase: 'cleanup' },
        { turn: 3, phase: 'untap' },
      ]),
    ).toEqual([
      {
        turn: 2,
        steps: [
          { phase: 'end', label: 'End' },
          { phase: 'cleanup', label: 'Cleanup' },
        ],
      },
      { turn: 3, steps: [{ phase: 'untap', label: 'Untap' }] },
    ])
  })

  it('keeps a position reached twice, twice', () => {
    // An extra combat phase (CR 506.1) revisits the combat steps inside one turn. Collapsing
    // them would shorten how far the game moved unasked.
    const runs = passedRuns([
      { turn: 4, phase: 'declare_blockers' },
      { turn: 4, phase: 'combat_damage' },
      { turn: 4, phase: 'declare_blockers' },
    ])
    expect(runs).toHaveLength(1)
    expect(runs[0]!.steps.map((step) => step.phase)).toEqual([
      'declare_blockers',
      'combat_damage',
      'declare_blockers',
    ])
  })

  it('starts a new run when the turn changes back', () => {
    const runs = passedRuns([
      { turn: 4, phase: 'end' },
      { turn: 5, phase: 'upkeep' },
      { turn: 4, phase: 'end' },
    ])
    expect(runs.map((run) => run.turn)).toEqual([4, 5, 4])
  })
})

describe('the whole preference, as a pace', () => {
  it('clears both lists to stop nowhere, which is what a bare message means', () => {
    // `docs/protocol.md`: the first `set_stops` a seat sends replaces the whole preference, and
    // a bare one means "stop nowhere". So the minimal message is the whole of this preset.
    expect(presetStops('nowhere')).toEqual({ type: 'set_stops' })
  })

  it('stops at the two main phases of your own turn, as the server seeds a human seat', () => {
    expect(presetStops('mains')).toEqual({
      type: 'set_stops',
      own_turn: ['precombat_main', 'postcombat_main'],
    })
  })

  it('claims every step, on every turn, as the way back from a skip', () => {
    const message = presetStops('everywhere')
    expect(message).toMatchObject({ type: 'set_stops', stops: PHASES })
    expect(message).not.toHaveProperty('own_turn')
  })

  it('reads the pace off the effective lists the server reflected', () => {
    expect(presetOf(view())).toBe('nowhere')
    expect(presetOf(view({ own_turn_stops: ['postcombat_main', 'precombat_main'] }))).toBe('mains')
    expect(presetOf(view({ stops: [...PHASES] }))).toBe('everywhere')
  })

  it('claims no pace at all for a preference edited step by step', () => {
    // Which is the honest answer: there is no client-held idea of a current pace, so a
    // preference that is none of the three simply matches none of them.
    expect(presetOf(view({ stops: ['end'] }))).toBeUndefined()
    expect(presetOf(view({ own_turn_stops: ['precombat_main'] }))).toBeUndefined()
  })
})
