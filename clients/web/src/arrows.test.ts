/**
 * Which stated relationships become arrows.
 *
 * The boundary worth pinning is what is *not* drawn: an attachment, because the card behind the
 * card already says it; an ability's source, because the stack item already carries that card;
 * and any edge whose other end the view did not name, because there is nothing to point at and
 * filling one in would be the client deciding the game.
 */
import { describe, expect, it } from 'vitest'

import { arrowsFor } from './arrows'
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
