import { readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { describe, expect, it } from 'vitest'

import { SpectatorView } from './protocol'
import { seats } from './table'
import { chairOf, watched, watchWording } from './watch'

const FIXTURES = join(
  dirname(fileURLToPath(import.meta.url)),
  '../../../crates/sage-protocol/fixtures',
)
const fixture = (name: string): SpectatorView =>
  SpectatorView.parse(JSON.parse(readFileSync(join(FIXTURES, name), 'utf8')))

/** Every field a seated view carries and a spectator's must not (`docs/protocol.md`). */
const RECEIVER_FIELDS = [
  'you',
  'me',
  'my_hand',
  'revealed',
  'mana_pool',
  'valid_actions',
  'action_deadline',
  'stops',
  'own_turn_stops',
  'auto_passed',
  'auto_passed_steps',
  'auto_passed_from',
  'action_rejected',
  'action_ack',
] as const

describe('a spectator view in the board’s shape', () => {
  it('renames `players` to `opponents` and passes the rest through', () => {
    const view = fixture('spectatorview.json')
    const board = watched(view)

    expect(board.opponents).toEqual(view.players)
    expect(board.battlefield).toEqual(view.battlefield)
    expect(board.stack).toEqual(view.stack)
    expect(board.graveyards).toEqual(view.graveyards)
    expect(board.log).toEqual(view.log)
    expect(board.player_names).toEqual(view.player_names)
    expect(board.phase).toBe(view.phase)
    expect(board.turn).toBe(view.turn)
    expect(board.active_player).toBe(view.active_player)
    expect(board.seat_order).toEqual(view.seat_order)
    expect(board.priority_player).toBe(view.priority_player)
    expect(board.format).toEqual(view.format)
  })

  it('carries the whole public half of a Commander game', () => {
    const view = fixture('spectatorview-commander.json')
    const board = watched(view)

    expect(board.command).toEqual(view.command)
    expect(board.exile).toEqual(view.exile)
    expect(board.emblems).toEqual(view.emblems)
    expect(board.commander_damage).toEqual(view.commander_damage)
    expect(board.commander_tax).toEqual(view.commander_tax)
    expect(board.commander_identity).toEqual(view.commander_identity)
    expect(board.result).toEqual(view.result)
  })

  it('invents no field the server did not send', () => {
    // The whole of the redaction on this side of the wire. A projection written field by field
    // could gain a `you` — and a spectator would be told a seat was theirs — so the test is on
    // the key set rather than on any one field.
    for (const name of ['spectatorview.json', 'spectatorview-commander.json']) {
      const view = fixture(name)
      const board = watched(view)
      const expected = new Set(
        Object.keys(view).map((key) => (key === 'players' ? 'opponents' : key)),
      )
      expect(new Set(Object.keys(board))).toEqual(expected)
      for (const field of RECEIVER_FIELDS) expect(board).not.toHaveProperty(field)
    }
  })

  it('leaves absence absent, so a missing pile is not an empty one', () => {
    // `spectatorview.json` states no exile, command, emblems or result. A projection that filled
    // those in with `[]` would make "the server sent no exile zone" indistinguishable from "the
    // exile zone is empty", which is the distinction `normalize.ts` exists to preserve.
    const board = watched(fixture('spectatorview.json'))
    expect(board).not.toHaveProperty('exile')
    expect(board).not.toHaveProperty('command')
    expect(board).not.toHaveProperty('emblems')
    expect(board).not.toHaveProperty('result')
  })

  it('names no seat as the reader’s, so nobody is called You', () => {
    // The seat a spectator sits behind is a chair, not a claim. `table.seats` marks nobody as
    // yours and `normalize.playerLabel` therefore names seats by position, never as an opponent.
    const table = seats(watched(fixture('spectatorview.json')))
    expect(table.every((seat) => !seat.isYou)).toBe(true)
    expect(table.map((seat) => seat.name)).toEqual(['Ari', 'Sam'])
    // Nobody's floating mana is on a spectator's wire, so no seat has any.
    expect(table.every((seat) => seat.manaPool.length === 0)).toBe(true)
  })
})

describe('the chair a spectator sits behind', () => {
  const table = seats(watched(fixture('spectatorview-commander.json')))

  it('is the first seat still in the game until one is chosen', () => {
    // `p0` is first in seat order and out of the game; the nearest half of the table is not
    // spent on a seat that controls nothing.
    expect(table[0]?.id).toBe('p0')
    expect(table[0]?.eliminated).toBe(true)
    expect(chairOf(table, undefined)?.id).toBe('p1')
  })

  it('is whichever seat was chosen, eliminated or not', () => {
    // Being out is not being gone: a spectator may want to read what a dead seat left behind.
    expect(chairOf(table, 'p2')?.id).toBe('p2')
    expect(chairOf(table, 'p0')?.id).toBe('p0')
  })

  it('falls back when the chosen seat is no longer at the table', () => {
    // A view that stopped naming a seat must not leave the near half of the board empty.
    expect(chairOf(table, 'p9')?.id).toBe('p1')
  })

  it('takes the first seat where every seat is out', () => {
    const allOut = table.map((seat) => ({ ...seat, eliminated: true }))
    expect(chairOf(allOut, undefined)?.id).toBe('p0')
  })

  it('is nothing at all at a table with no seats', () => {
    expect(chairOf([], 'p0')).toBeUndefined()
  })
})

describe('what the watching strip says', () => {
  const label = (id: string) => ({ p0: 'Ari', p1: 'Sam' })[id] ?? id

  it('names the seat the view said holds priority', () => {
    expect(watchWording(watched(fixture('spectatorview.json')), label)).toEqual({
      prompt: 'Watching — Ari to act',
      where: 'Turn 5 · Precombat main',
    })
  })

  it('claims nobody is being asked where the view named nobody', () => {
    // `priority_player` is absent while nobody holds it. Falling back to the active player
    // would state a thing the server did not.
    const quiet = { ...watched(fixture('spectatorview.json')) }
    delete quiet.priority_player
    expect(watchWording(quiet, label).prompt).toBe('Watching')
  })

  it('says a finished game is finished, whoever still holds priority', () => {
    const over = watched(fixture('spectatorview-commander.json'))
    expect(watchWording(over, label).prompt).toBe('Watching — the game is over')
    expect(watchWording(over, label).where).toBe('Turn 14 · Postcombat main')
  })
})
