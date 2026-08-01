import { describe, expect, it } from 'vitest'

import type { GameView, Permanent, StackItem } from './protocol'
import { relationLines, relations } from './relations'

const card = (id: string, name: string) => ({ id, name, type_line: 'Creature — Bear' })

const permanent = (id: string, extra: Partial<Permanent> = {}): Permanent => ({
  id,
  controller: 'p1',
  owner: 'p1',
  card: card(id, id),
  ...extra,
})

const view = (parts: Partial<GameView>): GameView => ({ phase: 'declare_blockers', ...parts })

describe('reading the board both ways', () => {
  it('gives an attacker the blockers that stopped it, which no field on it states', () => {
    // The whole reason this module exists. `blocking` rides on the blocker; the attacker
    // carries nothing at all about being blocked, and "how many blocked this one" is the
    // question combat is actually about.
    const all = relations(
      view({
        battlefield: [
          permanent('attacker', { attacking: true, attacking_player: 'p2' }),
          permanent('wall', { blocking: 'attacker' }),
          permanent('bear', { blocking: 'attacker' }),
        ],
      }),
    )

    expect(all.to('attacker').map((relation) => relation.from)).toEqual(['wall', 'bear'])
    expect(relationLines(all, 'attacker')).toEqual([
      { kind: 'attacking', direction: 'from', label: 'attacking', ids: ['p2'] },
      { kind: 'blocking', direction: 'to', label: 'blocked by', ids: ['wall', 'bear'] },
    ])
  })

  it('keeps two blockers as one line, in the order the server listed them', () => {
    // Multiple blockers are one relationship with two ends, not two relationships that happen
    // to share a phrase. The order is the view's — the client has no basis for another.
    const all = relations(
      view({
        battlefield: [
          permanent('bear', { blocking: 'attacker' }),
          permanent('wall', { blocking: 'attacker' }),
          permanent('attacker', { attacking: true }),
        ],
      }),
    )

    const blocked = relationLines(all, 'attacker').filter((line) => line.kind === 'blocking')
    expect(blocked).toHaveLength(1)
    expect(blocked[0]?.ids).toEqual(['bear', 'wall'])
  })

  it('names the planeswalker being attacked, not the seat that answers for it', () => {
    // The server sends both when a planeswalker is attacked: `attacking_player` is the seat
    // the attack is answered by, `attacking_planeswalker` is what the damage is going to.
    // Drawing the seat would point the arrow at the wrong object; drawing both would show one
    // attack as two.
    const all = relations(
      view({
        battlefield: [
          permanent('attacker', {
            attacking: true,
            attacking_player: 'p2',
            attacking_planeswalker: 'perm_walker',
          }),
        ],
      }),
    )

    expect(all.from('attacker')).toEqual([
      { kind: 'attacking', from: 'attacker', to: 'perm_walker' },
    ])
    expect(all.to('p2')).toEqual([])
  })

  it('states an attack whose defender the server did not name, without inventing one', () => {
    // `attacking: true` and nothing else is a fact about the game, and the honest rendering is
    // "attacking" with no name. Filling in the only opponent would be the client deciding
    // where an attack is going.
    const all = relations(view({ battlefield: [permanent('attacker', { attacking: true })] }))

    expect(all.from('attacker')).toEqual([{ kind: 'attacking', from: 'attacker', to: undefined }])
    expect(relationLines(all, 'attacker')).toEqual([
      { kind: 'attacking', direction: 'from', label: 'attacking', ids: [] },
    ])
    expect(all.linked('attacker').size).toBe(0)
  })

  it('reads an attachment from the thing attached and from the thing it is on', () => {
    const all = relations(
      view({
        battlefield: [permanent('aura', { attached_to: 'bear' }), permanent('bear')],
      }),
    )

    expect(relationLines(all, 'aura')).toEqual([
      { kind: 'attached', direction: 'from', label: 'attached to', ids: ['bear'] },
    ])
    expect(relationLines(all, 'bear')).toEqual([
      { kind: 'attached', direction: 'to', label: 'attached', ids: ['aura'] },
    ])
  })
})

describe('the stack, as relationships', () => {
  const stack: StackItem[] = [
    { id: 's1', controller: 'p1', description: 'Add {G}.', source: 'perm_bear', kind: 'ability' },
    {
      id: 's2',
      controller: 'p1',
      description: 'Twin Bolt',
      kind: 'spell',
      targets: [
        { kind: 'permanent', id: 'perm_bear' },
        { kind: 'player', player: 'p2' },
      ],
    },
    {
      id: 's3',
      controller: 'p2',
      description: 'Counterspell',
      kind: 'spell',
      targets: [{ kind: 'stack', id: 's2' }],
    },
  ]

  it('traces a target of every kind by the same entity id', () => {
    // Permanent, player, card, and stack object are four tags on the wire and one kind of
    // thing on screen: an id a click already reaches. A seat is a target like anything else.
    const all = relations(view({ stack }))

    expect(relationLines(all, 's2')).toEqual([
      { kind: 'targeting', direction: 'from', label: 'targeting', ids: ['perm_bear', 'p2'] },
      { kind: 'targeting', direction: 'to', label: 'targeted by', ids: ['s3'] },
    ])
    expect(relationLines(all, 'p2')).toEqual([
      { kind: 'targeting', direction: 'to', label: 'targeted by', ids: ['s2'] },
    ])
  })

  it('links an ability on the stack back to the permanent it came from', () => {
    const all = relations(view({ stack, battlefield: [permanent('perm_bear')] }))

    expect(relationLines(all, 's1')).toEqual([
      { kind: 'source', direction: 'from', label: 'from', ids: ['perm_bear'] },
    ])
    expect(relationLines(all, 'perm_bear')).toEqual([
      { kind: 'targeting', direction: 'to', label: 'targeted by', ids: ['s2'] },
      { kind: 'source', direction: 'to', label: 'on the stack', ids: ['s1'] },
    ])
  })

  it('gathers every end of every edge touching one object', () => {
    // What a focused view emphasises: one click on a spell should light up what it names and
    // what named it, in both directions and across zones.
    const all = relations(view({ stack }))

    expect([...all.linked('s2')].sort()).toEqual(['p2', 'perm_bear', 's3'])
  })
})

describe('a view with nothing to relate', () => {
  it('answers every question with nothing rather than failing', () => {
    const all = relations(view({}))

    expect(all.all).toEqual([])
    expect(all.from('anything')).toEqual([])
    expect(all.to('anything')).toEqual([])
    expect(all.linked('anything').size).toBe(0)
    expect(relationLines(all, 'anything')).toEqual([])
  })
})
