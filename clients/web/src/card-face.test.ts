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

import {
  cardFace,
  catalogFace,
  emblemFace,
  faceSummary,
  keywordLine,
  loyaltyCost,
  permanentFace,
  stackFace,
} from './card-face'
import { CatalogView, GameView, type CardView, type Permanent } from './protocol'

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
    const lathliss = permanentFace(permanent('gameview-commander.json', 'perm_lathliss'))

    expect(lathliss.markers).toContain('Commander')
    expect(lathliss.stat?.value).toBe('6/6')
  })

  it('names the colour a permanent chose as it entered, and says nothing when it chose none', () => {
    // CR 614.12: the choice is the permanent's, not the card's, so the board is the only
    // place it can be read. Nothing here works out what colour anything is — the letter is
    // stated and this only sets it in words.
    const bear = permanent('gameview.json', 'perm_bear')

    expect(permanentFace({ ...bear, chosen_color: 'R' }).markers).toContain('Chose Red')
    expect(permanentFace({ ...bear, chosen_color: 'W' }).markers).toContain('Chose White')
    expect(
      permanentFace({ ...bear, chosen_color: undefined }).markers.some((marker) =>
        marker.startsWith('Chose'),
      ),
    ).toBe(false)
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

describe('a card in the catalog', () => {
  const catalog = CatalogView.parse(
    JSON.parse(readFileSync(join(FIXTURES, 'catalogview.json'), 'utf8')),
  )
  const entry = (id: string) => {
    const found = catalog.cards?.find((card) => card.functional_id === id)
    if (!found) throw new Error(`the catalog fixture has no ${id}`)
    return found
  }

  it('is the same face as the card in a hand, addressed by identity', () => {
    // The server builds both projections from one place (ADR 0008 §7), so browsing Llanowar
    // Elves and drawing it must produce the same face — everything but the id, which is a
    // per-game entity in a hand and the card's own identity in a catalog.
    const browsed = catalogFace(entry('llanowar_elves'))
    const drawn = cardFace(view('gameview.json').my_hand![0]!)

    expect(browsed.id).toBe('llanowar_elves')
    expect(drawn.id).not.toBe(browsed.id)
    expect({ ...browsed, id: '' }).toEqual({ ...drawn, id: '' })
    // The identity doubles as the ADR 0012 art key, because that is the same handle.
    expect(browsed.artKey).toBe('llanowar_elves')
  })

  it('carries nothing a game would have added to it', () => {
    const angel = catalogFace(entry('serra_angel'))
    expect(angel.stat).toEqual({ kind: 'power_toughness', value: '4/4', label: 'Power/toughness' })
    expect(angel.keywords).toEqual(['flying', 'vigilance'])
    // No instance exists, so there is no tap state, no counter, and no marker to draw.
    expect(angel.tapped).toBe(false)
    expect(angel.counters).toEqual([])
    expect(angel.markers).toEqual([])
    expect(angel.damage).toBeUndefined()
    // A card with no generated rules text elides the field; an absence stays absent.
    expect(angel.rulesText).toBeUndefined()
  })

  it('leaves out what the entry did not state', () => {
    const forest = catalogFace(entry('forest'))
    expect(forest.manaCost).toBeUndefined()
    expect(forest.stat).toBeUndefined()
    expect(faceSummary(forest)).toBe('Forest · Basic Land — Forest')
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

describe('what the board adds to a card', () => {
  it('carries the colour identity the server stated, wherever the card is', () => {
    // The same fact on a card in hand, a permanent, and a catalog entry, so a deck builder and
    // a battlefield never draw one card two colours.
    const forest = cardFace({
      id: 'c1',
      name: 'Forest',
      type_line: 'Basic Land — Forest',
      color_identity: ['G'],
    })
    expect(forest.colorIdentity).toEqual(['G'])
    expect(forest.manaCost).toBeUndefined()
    // An absence stays an absence rather than becoming a claim about a colourless card.
    expect(cardFace({ id: 'c2', name: 'x', type_line: 'x' }).colorIdentity).toEqual([])
  })

  it('carries a granted keyword apart from the card’s printed text', () => {
    // A creature given trample for the turn has trample, and its card says nothing about
    // trample — so the words arrive separately and both are drawn. The client never works out
    // which is which; the server stated it (CR 613.1f).
    const pumped = permanentFace({
      id: 'perm_5',
      controller: 'p1',
      owner: 'p1',
      card: { id: 'c5', name: 'Bear', type_line: 'Creature — Bear', rules_text: '' },
      granted_keywords: ['Trample'],
    })
    expect(pumped.grantedKeywords).toEqual(['Trample'])
    // Said aloud as well as drawn: a reader that skipped it would describe a creature without
    // the ability it currently has.
    expect(faceSummary(pumped)).toContain('Trample')

    const plain = permanentFace({
      id: 'perm_6',
      controller: 'p1',
      owner: 'p1',
      card: { id: 'c6', name: 'Bear', type_line: 'Creature — Bear' },
    })
    expect(plain.grantedKeywords).toEqual([])
  })

  it('reports summoning sickness only where the server said so', () => {
    const bear = { id: 'c3', name: 'Bear', type_line: 'Creature — Bear' }
    const sick = permanentFace({
      id: 'perm_1',
      controller: 'p1',
      owner: 'p1',
      card: bear,
      summoning_sick: true,
    })
    expect(sick.summoningSick).toBe(true)
    expect(faceSummary(sick)).toContain('summoning sick')

    // Absent is "not stated", and a client that read it as "may attack" would be inventing a
    // rule it cannot see: haste may be granted, and control is stored engine state.
    const settled = permanentFace({ id: 'perm_2', controller: 'p1', owner: 'p1', card: bear })
    expect(settled.summoningSick).toBe(false)
    expect(faceSummary(settled)).not.toContain('summoning sick')
  })

  it('reports a skipped untap step only where the server said so', () => {
    const bear = { id: 'c4', name: 'Bear', type_line: 'Creature \u2014 Bear' }
    const held = permanentFace({
      id: 'perm_3',
      controller: 'p1',
      owner: 'p1',
      card: bear,
      tapped: true,
      skips_next_untap: true,
    })
    expect(held.skipsNextUntap).toBe(true)
    expect(faceSummary(held)).toContain("doesn't untap next untap step")

    // The spell that imposed it is gone, so absent has to mean "not stated" — a client that
    // guessed would be explaining a rule nobody told it about.
    const free = permanentFace({
      id: 'perm_4',
      controller: 'p1',
      owner: 'p1',
      card: bear,
      tapped: true,
    })
    expect(free.skipsNextUntap).toBe(false)
    expect(faceSummary(free)).not.toContain("doesn't untap")
  })
})

describe('a line of keywords', () => {
  const KEYWORDS = ['flying', 'haste', 'first strike', 'trample']

  it('is one whether the card printed one keyword or several', () => {
    expect(keywordLine('Flying', KEYWORDS)).toBe(true)
    expect(keywordLine('Flying, haste', KEYWORDS)).toBe(true)
    expect(keywordLine('Flying, first strike, trample', KEYWORDS)).toBe(true)
  })

  it('is one when the keyword itself has a space in it', () => {
    expect(keywordLine('First strike', KEYWORDS)).toBe(true)
  })

  it('is not one when anything on the line is not a keyword the server stated', () => {
    expect(keywordLine('Flying, menace', KEYWORDS)).toBe(false)
    expect(keywordLine('Whenever this creature attacks, draw a card.', KEYWORDS)).toBe(false)
    expect(keywordLine('', KEYWORDS)).toBe(false)
  })

  it('reads past reminder text and mana symbols, which are printed beside a keyword', () => {
    expect(keywordLine('Flying (This creature can only be blocked by fliers.)', KEYWORDS)).toBe(
      true,
    )
  })

  it('falls back to the older reading for a face the server stated no keywords for', () => {
    expect(keywordLine('Flying', [])).toBe(true)
    expect(keywordLine('Flying, haste', [])).toBe(false)
  })
})

describe('a loyalty ability', () => {
  it('splits the printed cost off the front of the line', () => {
    expect(loyaltyCost('+1: You gain 2 life.')).toEqual({ cost: '+1', rest: 'You gain 2 life.' })
    expect(loyaltyCost('0: Draw a card.')).toEqual({ cost: '0', rest: 'Draw a card.' })
    expect(loyaltyCost('−2: Deal 2 damage.')).toEqual({
      cost: '−2',
      rest: 'Deal 2 damage.',
    })
  })

  it('reads a hyphen as the minus the card prints, so the symbol still points down', () => {
    expect(loyaltyCost('-7: Take an extra turn.')?.cost).toBe('−7')
  })

  it('is not one where the line does not lead with a cost', () => {
    expect(
      loyaltyCost('Whenever you tap a Forest for mana, add an additional {G}.'),
    ).toBeUndefined()
    expect(loyaltyCost('Flying')).toBeUndefined()
    // A colon with no number in front of it is an activated ability, not a loyalty one.
    expect(loyaltyCost('{T}: Add {G}.')).toBeUndefined()
  })
})

/**
 * A card with two faces (CR 712, `docs/client-design.md` §6.7). The fields on a `CardView`
 * describe the side that is **up**; `other_face` is the side that is not, and both facts it
 * carries — that there is one, and what is on it — are things a client cannot work out.
 */
describe('a card with two faces', () => {
  const NICOL_BOLAS: CardView = {
    id: 'card_bolas',
    name: 'Nicol Bolas, the Ravager',
    type_line: 'Legendary Creature — Elder Dragon',
    mana_cost: '{1}{U}{B}{R}',
    power: '4',
    toughness: '4',
    functional_id: 'nicol_bolas_the_ravager',
    other_face: {
      name: 'Nicol Bolas, the Arisen',
      type_line: 'Legendary Planeswalker — Bolas',
      loyalty: '7',
      rules_text: '+2: Draw two cards.',
    },
  }

  it('draws the face that is up, and carries the other one whole', () => {
    const face = cardFace(NICOL_BOLAS)

    expect(face.name).toBe('Nicol Bolas, the Ravager')
    expect(face.stat).toEqual({ kind: 'power_toughness', value: '4/4', label: 'Power/toughness' })
    expect(face.otherFace?.name).toBe('Nicol Bolas, the Arisen')
    expect(face.otherFace?.rulesText).toBe('+2: Draw two cards.')
    // The same card and the same object, so the same entity id (`docs/protocol.md`).
    expect(face.otherFace?.id).toBe('card_bolas')
  })

  it('gives a back face no mana cost and no art of the front’s', () => {
    // CR 712.4a: a back face has no cost, so the title band's trailing slot is simply empty and
    // the name is fitted against the whole band — which the existing fitting does with no
    // special case, because the pips take a constant share of the drawing and there are none.
    const back = cardFace(NICOL_BOLAS).otherFace
    expect(back?.manaCost).toBeUndefined()
    // And no art key: a picture looked up by card identity is the *card's*, so drawing it
    // behind the other side's text would be inventing a face.
    expect(back?.artKey).toBeUndefined()
    expect(back?.stat).toEqual({ kind: 'loyalty', value: '7', label: 'Starting loyalty' })
  })

  it('says there is another side, and never what is on it', () => {
    // The board's mark is one glyph in the run of state marks; the word is here, because a
    // glyph is not readable. Naming the back would put a fact on the board that is not on it.
    const summary = faceSummary(cardFace(NICOL_BOLAS))
    expect(summary).toContain('has another face')
    expect(summary).not.toContain('Arisen')
  })

  it('says nothing of the kind about a card with one face', () => {
    // Absence is the whole of the signal: a client cannot tell a transforming card from an
    // ordinary one, so a card the server said nothing about has no mark and no other side.
    const forest = cardFace(view('gameview.json').my_hand![1]!)
    expect(forest.otherFace).toBeUndefined()
    expect(faceSummary(forest)).not.toContain('another face')
  })

  it('follows a permanent that has transformed, which carries its front face', () => {
    // Which side is up is the server's to say and never worked out here: a permanent that has
    // transformed carries its *front* face in the same field, and the board draws what it is
    // told is up.
    const transformed: Permanent = {
      id: 'perm_bolas',
      controller: 'p0',
      owner: 'p0',
      card: {
        ...NICOL_BOLAS,
        name: 'Nicol Bolas, the Arisen',
        type_line: 'Legendary Planeswalker — Bolas',
        mana_cost: undefined,
        power: undefined,
        toughness: undefined,
        other_face: {
          name: 'Nicol Bolas, the Ravager',
          type_line: 'Legendary Creature — Elder Dragon',
          mana_cost: '{1}{U}{B}{R}',
        },
      },
      counters: [{ kind: 'loyalty', count: 7 }],
    }
    const face = permanentFace(transformed)
    expect(face.name).toBe('Nicol Bolas, the Arisen')
    expect(face.stat).toEqual({ kind: 'loyalty', value: '7', label: 'Loyalty' })
    expect(face.otherFace?.name).toBe('Nicol Bolas, the Ravager')
    expect(face.otherFace?.manaCost).toBe('{1}{U}{B}{R}')
  })
})
