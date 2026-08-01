/**
 * The card-presentation model, against the committed fixtures.
 *
 * These run over the same JSON the Rust tests pin and the mirror-parity suite parses, so what
 * is asserted here is what a real server sends rather than a shape invented for a test. What
 * this suite is actually protecting is the small set of places where the honest rendering is
 * *not* the obvious one: printed loyalty against current loyalty, a token against a card, and
 * an absent field against a zero.
 */
import { readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { describe, expect, it } from 'vitest'

import { cardFace, emblemFace, faceSummary, permanentFace, stackFace } from './card-face'
import { GameView, type Permanent } from './protocol'

const FIXTURES = join(
  dirname(fileURLToPath(import.meta.url)),
  '../../../crates/sage-protocol/fixtures',
)

const view = (name: string): GameView =>
  GameView.parse(JSON.parse(readFileSync(join(FIXTURES, name), 'utf8')))

const permanent = (fixture: string, id: string): Permanent => {
  const found = view(fixture).battlefield?.find((p) => p.id === id)
  if (!found) throw new Error(`${fixture} has no permanent ${id}`)
  return found
}

describe('a card off the battlefield', () => {
  it('carries everything the server projected and nothing it did not', () => {
    const hand = view('gameview.json').my_hand ?? []
    const elves = cardFace(hand[0]!)

    expect(elves.name).toBe('Llanowar Elves')
    expect(elves.manaCost).toBe('{G}')
    expect(elves.typeLine).toBe('Creature — Elf Druid')
    expect(elves.rulesText).toBe('{T}: Add {G}.')
    expect(elves.stat).toEqual({ kind: 'power_toughness', value: '1/1', label: 'Power/toughness' })
    // Nothing about a board it is not on.
    expect(elves.tapped).toBe(false)
    expect(elves.counters).toEqual([])
    expect(elves.damage).toBeUndefined()
    expect(elves.markers).toEqual([])
  })

  it('leaves absent fields absent rather than filling them in', () => {
    // A basic land: no cost, no rules text, no P/T. A face that supplied a placeholder for any
    // of them would be stating something the server did not, and the client cannot tell "this
    // card has none" from "this frame did not carry it".
    const forest = cardFace(view('gameview.json').my_hand![1]!)

    expect(forest.name).toBe('Forest')
    expect(forest.manaCost).toBeUndefined()
    expect(forest.rulesText).toBeUndefined()
    expect(forest.stat).toBeUndefined()
    expect(forest.keywords).toEqual([])
  })

  it('reads a printed loyalty as a starting value, never a current one', () => {
    // In hand there is no counter to consult, so the printed number is all there is — and it
    // is labelled for what it is, so it is not read as a total the planeswalker has now.
    const ajani = permanent('gameview-emblem.json', 'perm_ajani')
    const inHand = cardFace(ajani.card)

    expect(inHand.stat).toEqual({ kind: 'loyalty', value: '4', label: 'Starting loyalty' })
  })
})

describe('a permanent on the battlefield', () => {
  it('shows current loyalty from the counter, not the number printed on the card', () => {
    // The distinction this whole model exists for. The card face says 5 because that is what
    // Nissa enters with; what she *has* is the `loyalty` counter, and on the board only the
    // counter can be the answer or a planeswalker spent down to 1 still reads as 5.
    const nissa = permanent('gameview.json', 'perm_nissa')
    expect(nissa.card.loyalty).toBe('5')

    const face = permanentFace(nissa)
    expect(face.stat).toEqual({ kind: 'loyalty', value: '5', label: 'Loyalty' })
    // ...and the counter is not then repeated below as though it were a second fact.
    expect(face.counters).toEqual([])
  })

  it('shows no loyalty at all when the board state does not state one', () => {
    // A planeswalker whose loyalty counter is missing has no current loyalty the client can
    // name, and the printed number is not a substitute for it.
    const nissa = permanent('gameview.json', 'perm_nissa')
    const face = permanentFace({ ...nissa, counters: undefined })

    expect(face.stat).toBeUndefined()
  })

  it('leads with power/toughness and keeps loyalty in the counter row when it has both', () => {
    // An animated planeswalker is a creature and a planeswalker at once. P/T leads, but its
    // loyalty must not vanish from the board on the way.
    const nissa = permanent('gameview.json', 'perm_nissa')
    const face = permanentFace({
      ...nissa,
      card: { ...nissa.card, power: '4', toughness: '4' },
    })

    expect(face.stat).toEqual({ kind: 'power_toughness', value: '4/4', label: 'Power/toughness' })
    expect(face.counters).toEqual([{ kind: 'loyalty', count: 5 }])
  })

  it('keeps counters, marked damage, and tap state as three separate facts', () => {
    const bear = permanentFace(permanent('gameview.json', 'perm_bear'))

    expect(bear.tapped).toBe(true)
    expect(bear.damage).toBe(1)
    expect(bear.counters).toEqual([{ kind: '+1/+1', count: 2 }])
    // Power/toughness arrives already computed; damage is never subtracted from it here.
    expect(bear.stat?.value).toBe('2/2')
  })

  it('treats no marked damage as nothing to draw rather than a zero', () => {
    const bear = permanent('gameview.json', 'perm_bear')
    expect(permanentFace({ ...bear, damage: 0 }).damage).toBeUndefined()
    expect(permanentFace({ ...bear, damage: undefined }).damage).toBeUndefined()
  })

  it('marks a token and withholds the art key it has no identity for', () => {
    // A token has an empty `functional_id` (CR 111), so passing one along would look to a
    // cache like a card the server failed to resolve rather than a card that does not exist.
    const thopter = permanentFace(permanent('gameview.json', 'perm_thopter'))

    expect(thopter.markers).toContain('Token')
    expect(thopter.artKey).toBeUndefined()
    expect(thopter.keywords).toEqual(['flying'])
    expect(thopter.manaCost).toBeUndefined()
  })

  it('passes the art key through for a card that has one', () => {
    expect(permanentFace(permanent('gameview.json', 'perm_bear')).artKey).toBe('grizzly_bears')
  })

  it('marks a commander', () => {
    const jedit = permanentFace(permanent('gameview-commander.json', 'perm_jedit'))

    expect(jedit.markers).toContain('Commander')
    expect(jedit.stat?.value).toBe('5/5')
  })

  it('carries keywords the server computed', () => {
    const ogre = permanentFace(permanent('gameview-emblem.json', 'perm_ogre'))

    expect(ogre.keywords).toEqual(['vigilance', 'trample', 'indestructible'])
    // The emblem's anthem is already in the number: 6/4, not the printed 4/2.
    expect(ogre.stat?.value).toBe('6/4')
  })
})

describe('an object on the stack', () => {
  it('renders the card face when one rides along, keyed by the stack object', () => {
    const bolt = stackFace(view('gameview.json').stack![0]!)

    // The id is the stack object's, because that is what a target and the inspector address.
    expect(bolt.id).toBe('s1')
    expect(bolt.name).toBe('Lightning Bolt')
    expect(bolt.rulesText).toBe('Lightning Bolt deals 3 damage to any target.')
    expect(bolt.markers).toContain('Spell')
  })

  it('falls back to the server-composed description when there is no card', () => {
    // An ability has no card behind it and the description is the whole of what there is.
    const ability = stackFace(view('gameview.json').stack![1]!)

    expect(ability.name).toBe('Add {G}.')
    expect(ability.typeLine).toBeUndefined()
    expect(ability.markers).toEqual(['Ability'])
  })

  it('distinguishes the finer ability kinds the server states', () => {
    const stack = view('gameview.json').stack!
    expect(stackFace(stack[4]!).markers).toEqual(['Activated ability'])
    expect(stackFace(stack[5]!).markers).toEqual(['Triggered ability'])
  })

  it('renders a kind it has no wording for rather than dropping it', () => {
    const item = { ...view('gameview.json').stack![1]!, kind: 'delayed' as never }
    expect(stackFace(item).markers).toEqual(['delayed'])
  })
})

describe('an emblem', () => {
  it('renders its abilities and claims no card it does not have', () => {
    const emblem = emblemFace(view('gameview-emblem.json').emblems![0]!)

    expect(emblem.name).toBe('Emblem')
    expect(emblem.markers).toEqual(['Emblem'])
    expect(emblem.rulesText).toContain('Creatures you control get +2/+2.')
    expect(emblem.rulesText).toContain('Creatures you control have indestructible.')
    // No card, so no cost, no type line, no art key, and nothing to tap.
    expect(emblem.manaCost).toBeUndefined()
    expect(emblem.typeLine).toBeUndefined()
    expect(emblem.artKey).toBeUndefined()
    expect(emblem.tapped).toBe(false)
  })

  it('survives an emblem with no abilities listed', () => {
    expect(emblemFace({ id: 'e0', controller: 'p0' }).rulesText).toBeUndefined()
  })
})

describe('the one-line summary', () => {
  it('says everything the frame draws, in one string', () => {
    const summary = faceSummary(permanentFace(permanent('gameview.json', 'perm_bear')))

    expect(summary).toContain('Grizzly Bears')
    expect(summary).toContain('Power/toughness 2/2')
    expect(summary).toContain('tapped')
    expect(summary).toContain('1 damage')
    expect(summary).toContain('2× +1/+1')
  })

  it('says only what is there for a sparse face', () => {
    expect(faceSummary(cardFace(view('gameview.json').my_hand![1]!))).toBe(
      'Forest · Basic Land — Forest',
    )
  })
})
