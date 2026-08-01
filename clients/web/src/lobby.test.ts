/**
 * The lobby joins, against the committed fixtures.
 *
 * What matters here is the boundary, not the copying. A directory row offers a command because
 * the server advertised it, and a seat's status is the flags the server sent — both are places
 * where a client could quietly start deciding things, so both are pinned.
 */
import { readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { describe, expect, it } from 'vitest'

import { awaiting, roster, tableLabel, tables } from './lobby'
import { LobbyView } from './protocol'

const FIXTURES = join(
  dirname(fileURLToPath(import.meta.url)),
  '../../../crates/sage-protocol/fixtures',
)

const view = (name: string): LobbyView =>
  LobbyView.parse(JSON.parse(readFileSync(join(FIXTURES, name), 'utf8')))

describe('the table directory', () => {
  it('reads occupancy, lifecycle, and watchers off the summaries', () => {
    const rows = tables(view('lobbyview-open.json'))
    expect(rows.map((row) => row.roomId)).toEqual(['r_310', 'r_311', 'r_312'])
    expect(rows[0]).toMatchObject({ filled: 1, seats: 2, open: 1, stateLabel: 'Gathering' })
    expect(rows[1]).toMatchObject({ open: 0, stateLabel: 'Gathering' })
    expect(rows[2]).toMatchObject({ spectators: 3, stateLabel: 'In progress' })
    // Omitted means zero, per `RoomSummary::spectators`.
    expect(rows[0]?.spectators).toBe(0)
  })

  it('labels an unnamed table by its format, and never invents one', () => {
    const rows = tables(view('lobbyview-open.json'))
    expect(rows[0]?.label).toBe('Kitchen table')
    expect(rows[1]?.label).toBe('starter-1v1')
    expect(tableLabel({ config: { seats: 2, game_setup: 'commander' } })).toBe('commander')
  })

  it('leads with join where there is a seat, and watch where there is not', () => {
    const rows = tables(view('lobbyview-open.json'))
    expect(rows.map((row) => row.reach)).toEqual([
      'join_room',
      // Full, so joining is not the thing on offer even though the room is still gathering.
      'spectate_room',
      'spectate_room',
    ])
  })

  it('offers nothing the server did not advertise', () => {
    // The same directory, read by a connection already seated: it is offered neither command,
    // so every row informs and none of them acts.
    const seated = { ...view('lobbyview-open.json'), valid_commands: ['submit_deck', 'leave'] }
    expect(tables(seated).map((row) => row.reach)).toEqual([undefined, undefined, undefined])

    // And a connection offered only `join_room` gets no watch button on a running game.
    const joiner = { ...view('lobbyview-open.json'), valid_commands: ['join_room'] }
    expect(tables(joiner).map((row) => row.reach)).toEqual(['join_room', undefined, undefined])
  })

  it('reads a view with no directory as no tables', () => {
    expect(tables({ you: 'p1' })).toEqual([])
  })
})

describe('the roster of a table', () => {
  const room = () => {
    const found = view('lobbyview.json').room
    if (!found) throw new Error('the fixture is not in a room')
    return found
  }

  it('names a seat by what the server called it, then by what it can fall back to', () => {
    const rows = roster(room(), 'p0', { random: 'Practice bot' })
    expect(rows.map((row) => row.label)).toEqual(['Ari', 'Practice bot'])

    // No stated name: an occupied seat falls back to its opaque id, an empty one to its index,
    // and an AI seat to the name the catalog advertised for its kind.
    const bare = roster(
      {
        room_id: 'r_1',
        config: { seats: 3, game_setup: 'starter-1v1' },
        seats: [{ seat: 0, occupied_by: 'p7' }, { seat: 1, ai: 'random' }, { seat: 2 }],
      },
      'p7',
      { random: 'Practice bot' },
    )
    expect(bare.map((row) => row.label)).toEqual(['p7', 'Practice bot', 'Seat 3'])

    // With no catalog fetched there is no advertised name, so the kind id is what there is.
    const uncatalogued = roster(
      {
        room_id: 'r_1',
        config: { seats: 2, game_setup: 'starter-1v1' },
        seats: [{ seat: 0, ai: 'random' }],
      },
      'p7',
    )
    expect(uncatalogued[0]?.label).toBe('random')
  })

  it('marks your own seat from the identity the view stated', () => {
    expect(roster(room(), 'p0').map((row) => row.you)).toEqual([true, false])
    expect(roster(room(), undefined).map((row) => row.you)).toEqual([false, false])
  })

  it('restates the seat flags rather than interpreting them', () => {
    const rows = roster(room(), 'p0')
    expect(rows[0]).toMatchObject({ decked: true, ready: true, awaiting: undefined })
    expect(rows[0]?.status).toEqual(['Deck submitted', 'Ready'])
    // The fixture's AI seat is decked but carries no `ready`, and that is reported as sent —
    // an AI seat is not given a readiness the server did not state.
    expect(rows[1]).toMatchObject({ ai: 'random', decked: true, ready: false })
    expect(rows[1]?.awaiting).toBe('Not ready')
  })

  it('says what each unset seat still owes, in seat order', () => {
    const rows = roster(
      {
        room_id: 'r_1',
        config: { seats: 3, game_setup: 'starter-1v1' },
        seats: [
          { seat: 0, occupied_by: 'p0', decked: true, ready: true },
          { seat: 1, occupied_by: 'p1' },
          { seat: 2 },
        ],
      },
      'p0',
    )
    expect(awaiting(rows)).toEqual(['Seat 2 — No deck yet', 'Seat 3 — Nobody here yet'])
    // Every seat set: nothing is waited on, which is a report and not a prediction that the
    // server is about to start the game.
    expect(awaiting(rows.slice(0, 1))).toEqual([])
  })

  it('reads a room with no seat roster as an empty one', () => {
    expect(roster({ room_id: 'r_1', config: { seats: 2, game_setup: 'x' } }, 'p0')).toEqual([])
  })
})
