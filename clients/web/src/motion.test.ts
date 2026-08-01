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

describe('a card followed between two zones', () => {
  const inHand = (id: string) => ({ id, name: 'Llanowar Elves', type_line: 'Creature — Elf Druid' })
  const asPermanent = (id: string, physical: string) => ({
    id,
    controller: 'p1',
    owner: 'p1',
    physical_card: physical,
    card: { id, name: 'Llanowar Elves', type_line: 'Creature — Elf Druid' },
  })
  const asSpell = (id: string, physical: string) => ({
    id,
    controller: 'p1',
    description: 'Llanowar Elves',
    kind: 'spell' as const,
    physical_card: physical,
  })

  it('joins the two ids the server said are of one physical card', () => {
    const before = view({ my_hand: [inHand('card_5')] })
    const after = view({ battlefield: [asPermanent('perm_9', 'card_5')] })

    expect(changes(before, after).flights).toEqual([
      { from: 'card_5', to: 'perm_9', card: 'card_5' },
    ])
  })

  it('follows one card across hand, stack, battlefield and graveyard', () => {
    // Each step is a join between two consecutive views and nothing else — no accumulated map,
    // no memory of the step before it. The ids differ every time, which is CR 400.7 and not a
    // gap: four objects, one physical card.
    const hand = view({ my_hand: [inHand('card_5')] })
    const stack = view({ stack: [asSpell('stack_2', 'card_5')] })
    const field = view({ battlefield: [asPermanent('perm_9', 'card_5')] })
    const grave = view({ graveyards: [{ player_id: 'p1', cards: [inHand('card_5')] }] })

    expect(changes(hand, stack).flights).toEqual([
      { from: 'card_5', to: 'stack_2', card: 'card_5' },
    ])
    expect(changes(stack, field).flights).toEqual([
      { from: 'stack_2', to: 'perm_9', card: 'card_5' },
    ])
    expect(changes(field, grave).flights).toEqual([
      { from: 'perm_9', to: 'card_5', card: 'card_5' },
    ])
  })

  it('tells two copies of one card apart, because the join is never by name', () => {
    // Both Forests agree on everything a client can see. Only one of them moved, and a join by
    // name would be a coin flip dressed up as a fact.
    const forest = (id: string) => ({ id, name: 'Forest', type_line: 'Basic Land — Forest' })
    const before = view({ my_hand: [forest('card_5'), forest('card_6')] })
    const after = view({
      my_hand: [forest('card_5')],
      battlefield: [
        {
          id: 'perm_9',
          controller: 'p1',
          owner: 'p1',
          physical_card: 'card_6',
          card: forest('perm_9'),
        },
      ],
    })

    expect(changes(before, after).flights).toEqual([
      { from: 'card_6', to: 'perm_9', card: 'card_6' },
    ])
  })

  it('says nothing about a projection that names no physical card', () => {
    // A token (CR 111) is not a card and an ability on the stack has none, so neither is
    // followable and neither is guessed at. They arrive; they do not fly.
    const token = {
      id: 'perm_t',
      controller: 'p1',
      owner: 'p1',
      card: { ...inHand('perm_t'), token: true },
    }
    const before = view({ my_hand: [inHand('card_5')] })
    const after = view({ my_hand: [inHand('card_5')], battlefield: [token] })

    const got = changes(before, after)
    expect(got.flights).toEqual([])
    expect([...got.arrived]).toEqual(['perm_t'])
  })

  it('does not also call the destination an arrival', () => {
    // The destination is a new id, so it qualifies as an arrival too — and playing both would
    // draw one event twice, a card popping into existence and then flying there.
    const before = view({ my_hand: [inHand('card_5')] })
    const after = view({ battlefield: [asPermanent('perm_9', 'card_5')] })

    expect(changes(before, after).arrived.size).toBe(0)
  })

  it('says nothing when a card stayed where it was', () => {
    const board = view({ battlefield: [asPermanent('perm_9', 'card_5')] })
    expect(changes(board, board).flights).toEqual([])
  })

  it('refuses to pick when one card is drawn in two places at once', () => {
    // Which of the two an animation should fly from is not something the view states, so this
    // states nothing either. Guessing quietly is the same mistake as joining by name.
    const before = view({ my_hand: [inHand('card_5')] })
    const after = view({
      battlefield: [asPermanent('perm_9', 'card_5')],
      stack: [asSpell('stack_2', 'card_5')],
    })

    expect(changes(before, after).flights).toEqual([])
  })

  it('says nothing on the first view of a game', () => {
    expect(
      changes(undefined, view({ battlefield: [asPermanent('perm_9', 'card_5')] })).flights,
    ).toEqual([])
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
