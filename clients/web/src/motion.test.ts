import { describe, expect, it } from 'vitest'

import { changes, lifeWording } from './motion'
import type { GameView } from './protocol'

const view = (parts: Partial<GameView>): GameView => ({ phase: 'precombat_main', ...parts })

const card = (id: string) => ({ id, name: id, type_line: 'Creature — Bear' })
const permanent = (id: string) => ({ id, controller: 'p1', owner: 'p1', card: card(id) })
const seats = (you: number, them: number): Partial<GameView> => ({
  you: 'p1',
  me: { life: you, library_size: 30 },
  opponents: [{ player_id: 'p2', life: them, hand_size: 5, library_size: 30, graveyard_size: 0 }],
})

describe('what arrived since the last view', () => {
  it('names an object this view draws and the last one did not', () => {
    const before = view({ battlefield: [permanent('perm_1')] })
    const after = view({ battlefield: [permanent('perm_1'), permanent('perm_2')] })

    expect([...changes(before, after).arrived]).toEqual(['perm_2'])
  })

  it('counts a card appearing in a pile, which is the half of a zone change the wire states', () => {
    // The card's own id, in the graveyard it is now in. This is a real appearance and needs no
    // join: what it deliberately is *not* is the permanent that died, whose id was different.
    const before = view({ graveyards: [{ player_id: 'p1', cards: [card('g1')] }] })
    const after = view({ graveyards: [{ player_id: 'p1', cards: [card('g1'), card('g2')] }] })

    expect([...changes(before, after).arrived]).toEqual(['g2'])
  })

  it('says nothing arrived when there is no previous view', () => {
    // The first frame of a game, and the first frame after a reconnect, are states a player is
    // arriving at rather than changes they watched. Animating them flashes the whole board at
    // the one moment it most needs to be read.
    expect(changes(undefined, view({ battlefield: [permanent('perm_1')] })).arrived.size).toBe(0)
  })

  it('says nothing about an object that left, because there is nothing left to draw', () => {
    const before = view({ battlefield: [permanent('perm_1'), permanent('perm_2')] })
    const after = view({ battlefield: [permanent('perm_1')] })

    expect(changes(before, after).arrived.size).toBe(0)
  })

  it('does not treat a re-ordered board as an arrival', () => {
    // The view's order is the server's and it can change for reasons that are not events.
    const before = view({ battlefield: [permanent('a'), permanent('b')] })
    const after = view({ battlefield: [permanent('b'), permanent('a')] })

    expect(changes(before, after).arrived.size).toBe(0)
  })
})

describe('what a seat’s life did', () => {
  it('states the change and its direction, for you and for them', () => {
    const got = changes(view(seats(20, 20)), view(seats(17, 22))).life

    expect(got.get('p1')).toBe(-3)
    expect(got.get('p2')).toBe(2)
  })

  it('says nothing about a total that did not move', () => {
    expect(changes(view(seats(20, 20)), view(seats(20, 18))).life.has('p1')).toBe(false)
  })

  it('says nothing about a seat only one of the two views states', () => {
    // A seat that has just appeared has not lost its whole life total, and one the view stopped
    // carrying has not gone to zero. Absence is a fact about what the server said.
    const before = view({ you: 'p1', me: { life: 20, library_size: 30 } })
    const after = view(seats(20, 20))

    expect(changes(before, after).life.size).toBe(0)
  })

  it('says which way it went in words, not only in a sign', () => {
    expect(lifeWording(-3)).toBe('lost 3 life')
    expect(lifeWording(2)).toBe('gained 2 life')
  })
})
