/**
 * Which stated relationships become arrows.
 *
 * The boundary worth pinning is what is *not* drawn: an attachment, because the card behind the
 * card already says it; an ability's source, because the stack item already carries that card;
 * and any edge whose other end the view did not name, because there is nothing to point at and
 * filling one in would be the client deciding the game.
 */
import { describe, expect, it } from 'vitest'

import { arrowsFor, draftArrows } from './arrows'
import { slotsOf } from './interaction'
import type { Relation } from './relations'

describe('arrowsFor', () => {
  it('draws combat in one tone and targeting in the other', () => {
    const relations: Relation[] = [
      { kind: 'attacking', from: 'perm_a', to: 'p2' },
      { kind: 'blocking', from: 'perm_b', to: 'perm_a' },
      { kind: 'targeting', from: 'stack_1', to: 'perm_a' },
    ]
    expect(arrowsFor(relations)).toEqual([
      { from: 'perm_a', to: 'p2', tone: 'combat' },
      { from: 'perm_b', to: 'perm_a', tone: 'combat' },
      { from: 'stack_1', to: 'perm_a', tone: 'target' },
    ])
  })

  it('draws neither an attachment nor an ability’s source', () => {
    const relations: Relation[] = [
      { kind: 'attached', from: 'perm_sword', to: 'perm_bear' },
      { kind: 'source', from: 'stack_1', to: 'perm_bear' },
    ]
    expect(arrowsFor(relations)).toEqual([])
  })

  it('drops an edge whose other end the view never named', () => {
    const relations: Relation[] = [{ kind: 'attacking', from: 'perm_a' }]
    expect(arrowsFor(relations)).toEqual([])
  })

  it('keeps the view’s own order', () => {
    const relations: Relation[] = [
      { kind: 'targeting', from: 's2', to: 'x' },
      { kind: 'attacking', from: 'p1', to: 'y' },
    ]
    expect(arrowsFor(relations).map((arrow) => arrow.from)).toEqual(['s2', 'p1'])
  })
})

describe('draftArrows', () => {
  const attackers = {
    id: 'a_attack',
    type: 'declare_attackers',
    label: 'Declare attackers',
    requirements: [
      { slot: 'attackers', prompt: 'Choose which creatures attack', candidates: ['perm_a'] },
      {
        slot: 'defend_a',
        prompt: 'Choose what Bear attacks',
        subject: 'perm_a',
        candidates: ['p2', 'perm_walker'],
      },
    ],
  }

  it('draws each attacker at what it was aimed at, while it is still a draft', () => {
    // The picture is the whole point of declaring one attacker at a time: three attackers
    // pointed at two defenders is a fact the words in the bar cannot hold.
    const slots = slotsOf(attackers, { attackers: ['perm_a'], defend_a: ['perm_walker'] })
    expect(draftArrows(attackers, slots)).toEqual([
      { from: 'perm_a', to: 'perm_walker', tone: 'combat' },
    ])
  })

  it('draws a spell’s targets from the spell, in the targeting tone', () => {
    const shock = {
      id: 'a_shock',
      type: 'cast_spell',
      label: 'Cast Shock',
      subject: ['c_shock'],
      requirements: [{ slot: 't0', prompt: 'any target', candidates: ['p1', 'perm_a'] }],
    }
    expect(draftArrows(shock, slotsOf(shock, { t0: ['p1'] }))).toEqual([
      { from: 'c_shock', to: 'p1', tone: 'target' },
    ])
  })

  it('draws nothing for a slot with no object to leave from', () => {
    // A global action with a target slot has no card to start the line at, and picking one
    // would be the client inventing where the arrow comes from.
    const global = {
      id: 'a_g',
      type: 'x',
      label: 'x',
      requirements: [{ slot: 't0', prompt: 'p', candidates: ['p1'] }],
    }
    expect(draftArrows(global, slotsOf(global, { t0: ['p1'] }))).toEqual([])
  })

  it('draws nothing at all when nothing is armed', () => {
    expect(draftArrows(undefined, [])).toEqual([])
  })
})
