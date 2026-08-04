import { describe, expect, it } from 'vitest'

import { costTint, frameTint, inlineSymbols, manaSymbols, spokenCost, spokenSymbol } from './mana'

describe('manaSymbols', () => {
  it('splits a printed cost into one symbol per pair of braces', () => {
    expect(manaSymbols('{2}{G}{G}').map((symbol) => symbol.printed)).toEqual(['{2}', '{G}', '{G}'])
  })

  it('is empty for a card the server sent no cost for', () => {
    // A land, a token, an ability on the stack. Absent stays absent: an empty row of pips is
    // not the same claim as "this costs nothing".
    expect(manaSymbols(undefined)).toEqual([])
    expect(manaSymbols('')).toEqual([])
  })

  it('reads a colour pip as the one colour it is drawn in', () => {
    expect(manaSymbols('{R}')).toEqual([
      { printed: '{R}', glyph: 'R', kind: 'color', colors: ['r'] },
    ])
  })

  it('reads generic and variable pips as uncoloured', () => {
    expect(manaSymbols('{10}{X}').map((symbol) => [symbol.kind, symbol.colors])).toEqual([
      ['generic', []],
      ['variable', []],
    ])
  })

  it('keeps both halves of a hybrid pip', () => {
    // Which half is actually paid is the server's answer; the pip shows what is printed.
    expect(manaSymbols('{G/U}')[0]).toEqual({
      printed: '{G/U}',
      glyph: 'G/U',
      kind: 'hybrid',
      colors: ['g', 'u'],
    })
  })

  it('reads a monocoloured hybrid as the one colour it names', () => {
    expect(manaSymbols('{2/W}')[0]).toMatchObject({ kind: 'hybrid', colors: ['w'] })
  })

  it('marks a Phyrexian pip as its own kind, keeping its colour', () => {
    expect(manaSymbols('{G/P}')[0]).toMatchObject({ kind: 'phyrexian', colors: ['g'] })
  })

  it('names the pips that are neither generic nor coloured', () => {
    expect(manaSymbols('{C}{S}{T}{Q}').map((symbol) => symbol.kind)).toEqual([
      'colorless',
      'snow',
      'tap',
      'untap',
    ])
  })

  it('keeps a symbol it does not recognise rather than dropping it', () => {
    // A newer server's card must stay legible here before this build knows what it is, and a
    // cost that is quietly one pip short is worse than one that looks unfamiliar.
    const [symbol] = manaSymbols('{½}')
    expect(symbol).toMatchObject({ printed: '{½}', glyph: '½', kind: 'other', colors: [] })
  })

  it('keeps text outside braces instead of discarding it', () => {
    expect(manaSymbols('2{R}').map((symbol) => symbol.glyph)).toEqual(['2', 'R'])
  })
})

describe('costTint', () => {
  it('is the one colour a monocoloured cost prints', () => {
    expect(costTint('{2}{U}')).toBe('u')
  })

  it('is multicolour once a cost prints more than one', () => {
    expect(costTint('{W}{U}')).toBe('multicolor')
    // From a single pip too: a hybrid card is a gold card to look at.
    expect(costTint('{G/U}')).toBe('multicolor')
  })

  it('repeats of one colour stay that colour', () => {
    expect(costTint('{G}{G}{G}')).toBe('g')
  })

  it('is neutral for anything with no coloured pip', () => {
    // A land, an artifact, and a card whose colour comes from somewhere the client cannot see
    // all land here. The tint is the printed pips and nothing else — a frame that guessed
    // would be asserting a characteristic the server never sent.
    expect(costTint(undefined)).toBe('colorless')
    expect(costTint('{4}')).toBe('colorless')
    expect(costTint('{C}')).toBe('colorless')
  })
})

describe('spokenCost', () => {
  it('says a cost the way a player says it', () => {
    expect(spokenCost('{2}{G}{G}')).toBe('2 green green mana')
  })

  it('says both halves of a composite pip', () => {
    expect(spokenCost('{G/U}')).toBe('green or blue hybrid mana')
    expect(spokenCost('{G/P}')).toBe('green Phyrexian mana')
  })

  it('says nothing at all for a card with no cost', () => {
    expect(spokenCost(undefined)).toBe('')
  })
})

describe('symbols inside a sentence', () => {
  it('splits rules text into prose and symbols', () => {
    expect(
      inlineSymbols('{T}: Add {G}.').map((t) => (t.kind === 'text' ? t.text : t.symbol.glyph)),
    ).toEqual(['T', ': Add ', 'G', '.'])
  })

  it('is all prose when there is nothing to draw', () => {
    expect(inlineSymbols('Flying')).toEqual([{ kind: 'text', text: 'Flying' }])
  })

  it('keeps prose outside braces as prose', () => {
    // The opposite tolerance from `manaSymbols`: that one reads a cost, where a bare run is a
    // malformed cost worth keeping; this reads a sentence, where a bare run is the sentence.
    const tokens = inlineSymbols('Pay 2 life: add {B}.')
    expect(tokens.filter((t) => t.kind === 'symbol')).toHaveLength(1)
  })

  it('leaves an empty pair of braces alone rather than drawing an empty disc', () => {
    expect(inlineSymbols('a {} b')).toEqual([
      { kind: 'text', text: 'a ' },
      { kind: 'text', text: '{}' },
      { kind: 'text', text: ' b' },
    ])
  })

  it('says a symbol the way it is read aloud', () => {
    const [tap] = inlineSymbols('{T}')
    expect(tap?.kind === 'symbol' && spokenSymbol(tap.symbol)).toBe('tap')
  })
})

describe('the wash a frame is drawn in', () => {
  it('is the printed pips whenever the card prints any', () => {
    expect(frameTint('{1}{G}')).toBe('g')
    expect(frameTint('{G}{U}')).toBe('multicolor')
    // The cost wins over the identity, so a card with an off-colour activated ability does not
    // change colour in a hand.
    expect(frameTint('{1}{G}', ['G', 'U'])).toBe('g')
  })

  it('falls back to the colour identity for a card the cost says nothing about', () => {
    // A Forest costs nothing and prints no coloured pip. Reading the cost alone made every
    // basic land the same grey, which is the least scannable part of a board to get wrong.
    expect(frameTint(undefined, ['G'])).toBe('g')
    expect(frameTint('{2}', ['R'])).toBe('r')
    expect(frameTint(undefined, ['G', 'U'])).toBe('multicolor')
  })

  it('stays neutral when the server stated neither', () => {
    expect(frameTint(undefined, [])).toBe('colorless')
    expect(frameTint('{2}')).toBe('colorless')
    // A letter this build does not know is not a colour it may invent one for.
    expect(frameTint(undefined, ['Z'])).toBe('colorless')
  })
})
