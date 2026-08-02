import { readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { describe, expect, it } from 'vitest'

import {
  isConnected,
  isNamed,
  list,
  playerLabel,
  powerToughness,
  seatOrder,
  seatSummary,
} from './normalize'
import { GameView } from './protocol'

const FIXTURES = join(
  dirname(fileURLToPath(import.meta.url)),
  '../../../crates/sage-protocol/fixtures',
)
const view = (name: string) =>
  GameView.parse(JSON.parse(readFileSync(join(FIXTURES, name), 'utf8')))

describe('absence', () => {
  it('reads an absent collection and an empty one the same way', () => {
    // The server elides an empty list, but one can also arrive explicitly empty — a finished
    // game carries `valid_actions: []`. Both mean "nothing", and a screen that told them apart
    // would show a concluded game as still offering moves.
    expect(list(undefined)).toEqual([])
    expect(list([])).toEqual([])
    expect(list(view('gameview-over.json').valid_actions)).toEqual([])
  })

  it('reads an absent `connected` as connected', () => {
    // The flag rides the wire only when false, so absence is the common case. Reading it as
    // false would paint every healthy seat as dropped.
    expect(isConnected({})).toBe(true)
    expect(isConnected({ connected: false })).toBe(false)
    expect(isConnected({ connected: true })).toBe(true)
  })
})

describe('naming a seat', () => {
  it('says the server’s name and nothing beside it', () => {
    // The whole of the fix for #692: a seat id is a wire key, and printing it after the name a
    // player chose is `docs/client-design.md` §2.1 rule 3 — an identifier a player has no use
    // for — on every surface that names a seat at once.
    const board = view('gameview-board.json')
    expect(playerLabel(board, 'p1')).toBe('Ada')
    expect(playerLabel(board, 'p2')).toBe('Bo')
  })

  it('calls the seats of a table nobody named `You` and `Opponent`', () => {
    // `player_names` is elided when empty, so a nameless table is ordinary rather than exotic —
    // most committed fixtures are one. The view states which seat is the reader's, which is the
    // fact a player already uses to think about a two-seat table, so it is the one drawn.
    const table = { phase: 'untap' as const, you: 'p0', seat_order: ['p0', 'p1'] }
    expect(playerLabel(table, 'p0')).toBe('You')
    expect(playerLabel(table, 'p1')).toBe('Opponent')
  })

  it('tells several nameless opponents apart by where they sit', () => {
    // More than one other seat and no names: `Opponent` twice would be two seats with one label,
    // and the trail would name neither. The number is the seat's own position — a thing a player
    // can see — and never the id, which is what this whole function exists to keep off screen.
    const table = { phase: 'untap' as const, you: 'p0', seat_order: ['p0', 'p1', 'p2'] }
    expect(playerLabel(table, 'p0')).toBe('You')
    expect(playerLabel(table, 'p1')).toBe('Opponent 2')
    expect(playerLabel(table, 'p2')).toBe('Opponent 3')
  })

  it('claims no opponent for a reader the view seated nowhere', () => {
    // A view that named no seat as this reader's — a spectator — has no opponents in it. Calling
    // somebody else's opponent yours is a fact the view did not state, so only position is left.
    const table = { phase: 'untap' as const, seat_order: ['p0', 'p1'] }
    expect(playerLabel(table, 'p0')).toBe('Seat 1')
    expect(playerLabel(table, 'p1')).toBe('Seat 2')
  })

  it('reads a nameless seat the same way whether the order was stated or reconstructed', () => {
    // Half the fixtures carry no `seat_order`, and the label must not depend on which of the two
    // sources the seats came from — the panel, the field and the trail all read this one string.
    const reconstructed = {
      phase: 'untap' as const,
      you: 'p0',
      opponents: [{ player_id: 'p1', life: 20, library_size: 40, hand_size: 5, graveyard_size: 0 }],
    }
    expect(playerLabel(reconstructed, 'p0')).toBe('You')
    expect(playerLabel(reconstructed, 'p1')).toBe('Opponent')
  })

  it('disambiguates two seats the server let share a name, by seat and never by id', () => {
    // Reachable on purpose: `sage-server`'s `validate_name` allows a collision and says two
    // Alices are told apart by their seat. So the label has to survive it — with the position,
    // which a player can see, rather than the wire key, which is the defect being fixed.
    const table = {
      phase: 'untap' as const,
      you: 'p0',
      seat_order: ['p0', 'p1', 'p2'],
      player_names: { p0: 'Alice', p1: 'Alice', p2: 'Bob' },
    }
    expect(playerLabel(table, 'p0')).toBe('Alice (seat 1)')
    expect(playerLabel(table, 'p1')).toBe('Alice (seat 2)')
    // The seat that shares with nobody is left alone: a qualifier nobody needs is one more
    // thing to read.
    expect(playerLabel(table, 'p2')).toBe('Bob')
  })

  it('never prints the id, for any seat of any committed fixture', () => {
    // The sweep the browser guard runs, at the unit level and across every fixture: whatever a
    // seat ends up called, the one thing it may not contain is the string the wire calls it.
    for (const name of ['gameview.json', 'gameview-board.json', 'gameview-commander.json']) {
      const parsed = view(name)
      for (const id of seatOrder(parsed)) {
        expect(playerLabel(parsed, id).split(/[^A-Za-z0-9_]+/)).not.toContain(id)
      }
    }
  })

  it('says whether a seat’s name is the server’s word or the client’s', () => {
    // What the panel reads to decide whether `(you)` adds anything: a named seat needs it, and a
    // seat already called `You` would otherwise read `You (you)`.
    expect(isNamed(view('gameview-board.json'), 'p1')).toBe(true)
    expect(isNamed(view('gameview.json'), 'p1')).toBe(false)
  })
})

describe('seating', () => {
  it('prefers the order the server stated', () => {
    expect(seatOrder({ phase: 'untap', you: 'p1', seat_order: ['p0', 'p1'] })).toEqual(['p0', 'p1'])
  })

  it('reconstructs an order from the seats it knows when none was stated', () => {
    // Better than a table with nobody at it — and it invents no order for seats the view did not
    // list, which is why `you` comes first and the opponents follow in the order they arrived.
    expect(
      seatOrder({
        phase: 'untap',
        you: 'p0',
        opponents: [
          { player_id: 'p1', life: 20, library_size: 40, hand_size: 5, graveyard_size: 0 },
          { player_id: 'p0', life: 20, library_size: 40, hand_size: 5, graveyard_size: 0 },
        ],
      }),
    ).toEqual(['p0', 'p1'])
  })
})

describe('displayed characteristics', () => {
  it('shows power and toughness together, or not at all', () => {
    expect(powerToughness({ power: '2', toughness: '2' })).toBe('2/2')
    expect(powerToughness({})).toBeUndefined()
    // A land has neither; a half-present pair is not a creature to display.
    expect(powerToughness({ power: '2' })).toBeUndefined()
    // Which stat a face leads with — P/T, loyalty, or nothing — is `card-face.ts`, because
    // the answer differs between the battlefield and everywhere else and a single string
    // helper could not tell them apart.
  })

  it('summarizes a seat without inventing anything', () => {
    const summary = seatSummary({ life: 18, library_size: 33, hand_size: 5, graveyard_size: 2 })
    expect(summary).toContain('18 life')
    expect(summary).toContain('5 hand')
    expect(summary).not.toContain('disconnected')
  })

  it('names the states that matter', () => {
    const summary = seatSummary({
      life: 0,
      library_size: 0,
      eliminated: true,
      connected: false,
      ai: true,
    })
    expect(summary).toContain('eliminated')
    expect(summary).toContain('disconnected')
    expect(summary).toContain('AI')
  })
})
