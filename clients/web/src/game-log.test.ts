/**
 * The log's wording and its reading classes.
 *
 * The load-bearing test is the coverage one: every event the mirror declares must have both a
 * sentence and a class. Both fall back gracefully at runtime — a newer server's event says so
 * rather than vanishing — but a fallback reached by an event *this build declares* is a gap in
 * this build, not tolerance of a newer one, and it should fail here rather than show up in a
 * player's log as "(unrecognized log event)".
 */
import { describe as suite, expect, it } from 'vitest'

import { GameLogEvent, type GameLogEvent as Event } from './protocol'
import { describe, kindOf } from './game-log'

/** Every `type` literal the mirror declares, read off the schema rather than listed again. */
const TYPES: readonly string[] = GameLogEvent.options.map(
  (option) => option.shape.type.value as string,
)

const name = (id: string) => (id === 'p0' ? 'You' : 'Alice')
const card = { id: 'c1', name: 'Lightning Bolt' }

/** One event of each declared type, so coverage is asserted against real values. */
const SAMPLES: readonly Event[] = [
  { type: 'spell_cast', player: 'p0', card },
  { type: 'spell_resolved', player: 'p0', card },
  { type: 'spell_countered', player: 'p0', card },
  { type: 'spell_fizzled', player: 'p0', card },
  { type: 'attackers_declared', player: 'p0', attackers: [card] },
  { type: 'blockers_declared', player: 'p1', blocks: [{ blocker: card, attacker: card }] },
  { type: 'mulligan', player: 'p0' },
  { type: 'hand_kept', player: 'p0' },
  { type: 'life_changed', player: 'p0', amount: -3 },
  { type: 'damage_dealt', target: { kind: 'player', player: 'p1' }, amount: 3 },
  { type: 'cards_drawn', player: 'p0', count: 1 },
  { type: 'cards_milled', player: 'p0', count: 2 },
  { type: 'cards_exiled', player: 'p1', count: 3 },
  { type: 'cards_discarded', player: 'p0', count: 1 },
  { type: 'library_searched', player: 'p0' },
  { type: 'optional_applied', player: 'p0' },
  { type: 'optional_declined', player: 'p0' },
  { type: 'permanent_died', permanent: card },
  { type: 'step_changed', turn: 3, active_player: 'p0', phase: 'upkeep' },
  { type: 'player_eliminated', player: 'p1', reason: 'life_zero' },
  { type: 'commander_returned_to_command_zone', player: 'p0', card },
  { type: 'game_over', result: { reason: 'concede', winner: 'p0' } },
]

suite('the log', () => {
  it('has a sentence and a class for every event the mirror declares', () => {
    expect(SAMPLES.map((event) => event.type).sort()).toEqual([...TYPES].sort())
    for (const event of SAMPLES) {
      expect(describe(event, name), event.type).not.toContain('unrecognized')
      expect(kindOf(event), event.type).not.toBe('other')
    }
  })

  it('divides the column at a step change and weights the outcome', () => {
    expect(kindOf({ type: 'step_changed', turn: 3, active_player: 'p0', phase: 'upkeep' })).toBe(
      'step',
    )
    expect(kindOf({ type: 'game_over', result: { reason: 'decked' } })).toBe('result')
    expect(
      kindOf({ type: 'damage_dealt', target: { kind: 'player', player: 'p1' }, amount: 3 }),
    ).toBe('life')
  })

  it('says so rather than guessing when a newer server logs something unknown', () => {
    const future = { type: 'planar_die_rolled', player: 'p0' } as unknown as Event
    expect(describe(future, name)).toContain('unrecognized')
    expect(kindOf(future)).toBe('other')
  })
})
