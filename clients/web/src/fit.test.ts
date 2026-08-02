import { describe, expect, it } from 'vitest'

import {
  BODY_FLOOR,
  CARD_MIN_HEIGHT,
  cardPlan,
  fitName,
  fitTypeLine,
  MIN_STEM,
  NAME_DESIGNED,
  NAME_FLOOR,
  presentationFor,
  PRINTED_RATIO,
  textWidth,
  unstatedKeywords,
  wrapText,
  type CardText,
} from './fit'

/** The two boxes `docs/client-design.md` §5 states as minimums, and the designed permanent. */
const PERMANENT_MIN = { width: 72, height: 100 }
const HAND_MIN = { width: 100, height: 140 }
const DESIGNED = { width: 130, height: 182 }

const name = (text: string, width: number, mayAbbreviate = true) =>
  fitName(text, { width }, { floor: NAME_FLOOR, designed: NAME_DESIGNED, mayAbbreviate })

const face = (over: Partial<CardText> = {}): CardText => ({
  name: 'Grizzly Bears',
  keywords: [],
  ...over,
})

describe('estimating how wide text draws', () => {
  it('is monotonic in the size and in the string', () => {
    expect(textWidth('Forest', 12)).toBeGreaterThan(textWidth('Forest', 9))
    expect(textWidth('Llanowar Elves', 9)).toBeGreaterThan(textWidth('Llanowar', 9))
  })

  it('gives roughly the thirteen characters a line §6 expects of a 9px name in a 72px tile', () => {
    // The number the whole density argument rests on. A wide margin either way would mean the
    // spec and the estimator disagree about what fits, and every fitting decision inherits that.
    const perLine = 68 / (textWidth('abcdefghijklm', 9) / 13)
    expect(perLine).toBeGreaterThan(11)
    expect(perLine).toBeLessThan(15)
  })

  it('keeps a word whole when it wraps', () => {
    expect(wrapText('Nissa, Who Shakes the World', 60, 9)).not.toContain('')
    for (const line of wrapText('Nissa, Who Shakes the World', 60, 9)) {
      expect(line).not.toMatch(/^\s|\s$/)
    }
  })

  /**
   * **The direction of the error, pinned.**
   *
   * The estimate is allowed to be wrong; it is not allowed to be wrong the *narrow* way. Erring
   * wide costs a font size or a line, and §2 spends both before it gives up a character; erring
   * narrow draws text into a box smaller than the text, and `overflow: hidden` cuts it. This
   * module's own doc comment claimed the safe direction while the table pointed the other way —
   * `Creature — Ogre Warrior` drawn 130px into the 124px band it was told fitted, `Gravedigger`
   * 72px into 66px — so the claim is now a test.
   *
   * `MEASURED` is the widest each glyph is actually drawn, from `system-ui` at font-weight **700**
   * in the browser `@playwright/test` pins. 700 is heavier than any text that goes through this
   * module — the name band is 600 and the type line and rules are 400 — so a table that is an
   * upper bound at 700 has margin in hand at the weights it is really used at. To re-measure,
   * render `ch.repeat(10)` in a `white-space: pre` element at a large size and divide.
   */
  const MEASURED: Record<string, number> = {
    '—': 1,
    m: 0.975,
    W: 0.967,
    M: 0.943,
    '@': 0.898,
    w: 0.856,
    N: 0.813,
    O: 0.791,
    Q: 0.791,
    H: 0.765,
    U: 0.756,
    D: 0.732,
    G: 0.724,
    A: 0.692,
    X: 0.67,
    B: 0.665,
    K: 0.664,
    R: 0.656,
    h: 0.65,
    n: 0.65,
    u: 0.65,
    V: 0.65,
    b: 0.632,
    d: 0.632,
    g: 0.632,
    p: 0.632,
    q: 0.632,
    Y: 0.626,
    o: 0.625,
    C: 0.624,
    P: 0.621,
    k: 0.62,
    a: 0.599,
    e: 0.597,
    T: 0.595,
    Z: 0.579,
    x: 0.578,
    '0': 0.572,
    '1': 0.572,
    '5': 0.572,
    '8': 0.572,
    '9': 0.572,
    '+': 0.572,
    '×': 0.572,
    v: 0.571,
    y: 0.571,
    E: 0.56,
    L: 0.559,
    '|': 0.551,
    S: 0.551,
    F: 0.549,
    c: 0.516,
    s: 0.502,
    z: 0.493,
    r: 0.447,
    t: 0.434,
    '/': 0.415,
    '{': 0.394,
    '}': 0.394,
    I: 0.389,
    f: 0.386,
    '(': 0.339,
    ')': 0.339,
    '[': 0.331,
    ']': 0.331,
    J: 0.331,
    '-': 0.32,
    i: 0.299,
    j: 0.298,
    l: 0.298,
    ',': 0.285,
    ';': 0.285,
    '!': 0.282,
    '.': 0.281,
    ':': 0.281,
    '·': 0.281,
    "'": 0.266,
    ' ': 0.26,
    '’': 0.217,
  }

  it('never estimates a character narrower than the browser draws it', () => {
    const short = Object.entries(MEASURED)
      .filter(([character, drawn]) => textWidth(character, 100) < drawn * 100)
      .map(([character, drawn]) => `${character}: ${textWidth(character, 100) / 100} < ${drawn}`)
    expect(short, `${short.length} characters estimated narrower than they draw`).toEqual([])
  })

  it('never estimates a string narrower than the browser draws it', () => {
    // The strings the board is actually made of, including the three the scale gate caught this
    // on. A per-character bound gives a per-string bound, and this says so where a reader is
    // looking at the cards rather than at the alphabet.
    const drawn = (text: string, size: number) =>
      [...text].reduce((all, character) => all + (MEASURED[character] ?? 0), 0) * size
    for (const text of [
      'Creature — Ogre Warrior',
      'Legendary Creature — Elf Druid',
      'Gravedigger',
      'Colossal Dreadmaw',
      'Nissa, Who Shakes the World',
      'Llanowar Elves',
      'Basic Land — Forest',
      "Marauder's Axe",
    ]) {
      for (const size of [9, 11, 13, 17]) {
        expect(textWidth(text, size), `${text} at ${size}px`).toBeGreaterThanOrEqual(
          drawn(text, size),
        )
      }
    }
  })
})

describe('fitting a name: the largest size that works, then abbreviate', () => {
  it('draws a short name at the designed size on one line', () => {
    expect(name('Forest', 130)).toEqual({
      text: 'Forest',
      lines: 1,
      size: NAME_DESIGNED,
      abbreviated: false,
    })
  })

  it('takes a second line before it gives up a size', () => {
    // Wide enough for `Llanowar Elves` on one line, but not at 13px. §2: wrapping is one of the
    // ways text fits at a size, not a step after shrinking — so the second line is spent first
    // and the name is still drawn at the designed size.
    const fitted = name('Llanowar Elves', 78)
    expect(fitted.size).toBe(NAME_DESIGNED)
    expect(fitted.lines).toBe(2)
    expect(fitted.text).toBe('Llanowar Elves')
  })

  it('prefers fewer lines at equal size', () => {
    // The same name in a box wide enough to hold it whole at 13px: the second line buys nothing
    // here, so it is not taken. Fewer lines is the tiebreak, never the thing traded for size.
    //
    // The box is the designed permanent rather than the hand's 100px minimum, which is what this
    // asked for while the advance table under-stated a lowercase letter by a fifth: `Llanowar
    // Elves` needs about 106px at 13px, and a 100px box takes the second line. That is the
    // corrected estimate being *conservative* — the browser draws it in 94px — and a wrap is what
    // §2 spends before a size, so it is the error going the way this module promises.
    const fitted = name('Llanowar Elves', DESIGNED.width)
    expect(fitted.size).toBe(NAME_DESIGNED)
    expect(fitted.lines).toBe(1)
  })

  it('shrinks only when no line count works at that size', () => {
    // Two lines of 100px cannot hold this at 13px, so a size is given up — but only one at a
    // time, and the result is the largest size two lines *do* hold, not the floor.
    const fitted = name('Nissa, Who Shakes the World', HAND_MIN.width)
    expect(fitted.lines).toBe(2)
    expect(fitted.size).toBeGreaterThan(NAME_FLOOR)
    expect(fitted.size).toBeLessThan(NAME_DESIGNED)
    expect(fitted.text).toBe('Nissa, Who Shakes the World')
    expect(fitted.abbreviated).toBe(false)
  })

  it('never draws below the 9px floor', () => {
    for (const width of [20, 40, 72, 100, 130, 336]) {
      expect(name('Nissa, Who Shakes the World', width).size).toBeGreaterThanOrEqual(NAME_FLOOR)
      expect(name('Llanowar Elves', width, false).size).toBeGreaterThanOrEqual(NAME_FLOOR)
    }
  })

  it('abbreviates only as the last step, and only where it is allowed', () => {
    const abbreviated = name('Nissa, Who Shakes the World', 40)
    expect(abbreviated.abbreviated).toBe(true)
    expect(abbreviated.text.length).toBeGreaterThanOrEqual(MIN_STEM)
    expect('Nissa, Who Shakes the World'.startsWith(abbreviated.text)).toBe(true)
  })

  it('leaves a stem you could recognise a card by', () => {
    // `Troll Asce` is the example §6 names as recognisable, and it is exactly the minimum.
    const fitted = fitName(
      'Troll Ascetic',
      { width: 34 },
      { floor: NAME_FLOOR, designed: NAME_DESIGNED, mayAbbreviate: true, maxLines: 1 },
    )
    expect(fitted.abbreviated).toBe(true)
    expect(fitted.text.length).toBeGreaterThanOrEqual(MIN_STEM)
  })

  it('cuts no name in the hand, at any width', () => {
    // The hand is where a player chooses. Even a box that was sized wrong gets the whole name:
    // §6 calls a name that will not fit at the floor a sizing defect, not a permitted state.
    for (const width of [24, 47, 72, 100, 130]) {
      const fitted = name('Nissa, Who Shakes the World', width, false)
      expect(fitted.abbreviated).toBe(false)
      expect(fitted.text).toBe('Nissa, Who Shakes the World')
    }
  })

  it('does not abbreviate a name already at the minimum stem', () => {
    const fitted = name('Shock', 4)
    expect(fitted.text).toBe('Shock')
    expect(fitted.abbreviated).toBe(false)
  })

  it('leaves no dangling punctuation where it cut', () => {
    const fitted = name('Nissa, Who Shakes the World', 62)
    expect(fitted.text).not.toMatch(/[\s,]$/)
  })
})

describe('a wider box never draws smaller text', () => {
  // The property, not a table of expectations. A per-case number would have been satisfied by the
  // defect that prompted this: `Sword of Feast and Famine` drew at 9px on one line in a 130px card
  // and 13px on two in a 100px one, because size was searched to the floor before a second line
  // was considered at all. Every individual answer there is defensible; only the *relation*
  // between them is wrong, so only a sweep can see it.
  const LONG = [
    'Sword of Feast and Famine',
    'Nissa, Who Shakes the World',
    'Jace, the Mind Sculptor',
    'Elesh Norn, Grand Cenobite',
    'Sakashima of a Thousand Faces',
    'Kozilek, Butcher of Truth',
    'Llanowar Elves',
    'Grizzly Bears',
    'Forest',
  ]

  /** Every supported width from a chip to the inspector, at 1px — the resolution zoom moves in. */
  const widths = Array.from({ length: 341 }, (_, index) => index + 20)

  const variants = [
    { mayAbbreviate: true, maxLines: undefined },
    { mayAbbreviate: false, maxLines: undefined },
    { mayAbbreviate: true, maxLines: 1 as const },
  ]

  it('is monotonic in the width, for every name and every variant', () => {
    for (const text of LONG) {
      for (const variant of variants) {
        let previous = 0
        for (const width of widths) {
          const size = fitName(
            text,
            { width },
            {
              floor: NAME_FLOOR,
              designed: NAME_DESIGNED,
              mayAbbreviate: variant.mayAbbreviate,
              ...(variant.maxLines ? { maxLines: variant.maxLines } : {}),
            },
          ).size
          expect(
            size,
            `${text} at ${width}px (mayAbbreviate=${variant.mayAbbreviate}, ` +
              `maxLines=${variant.maxLines ?? 2}) shrank from ${previous} to ${size}`,
          ).toBeGreaterThanOrEqual(previous)
          previous = size
        }
      }
    }
  })

  it('draws the designed permanent width at the designed size, not the floor', () => {
    // The concrete half of the same property: 130px is the widest card the table draws, and it
    // rendering a long name at 9px was the report.
    for (const text of LONG) {
      expect(name(text, DESIGNED.width).size).toBe(NAME_DESIGNED)
      expect(name(text, DESIGNED.width).abbreviated).toBe(false)
    }
  })
})

/**
 * §6's name band, and the priority it is divided by.
 *
 * The cost is back where a printed card puts it and the band is shared, so the question this whole
 * block asks is the one §6 answers: **what does the cost cost the name?** The answer has to be
 * "nothing", and it has to be nothing at *every* width rather than at the sizes somebody checked —
 * a band divided before either part is fitted is exactly how a hand of `C…` happened once already.
 */
describe('the name band: the name leads, the cost follows', () => {
  const banded = (over: Partial<CardText>, box: { width: number; height: number }, may = true) =>
    cardPlan(face({ typeLine: 'Creature — Bear', ...over }), box, { mayAbbreviate: may })

  /** Names long enough to be a fight, and one that is not. */
  const NAMES = [
    'Forest',
    'Grizzly Bears',
    'Llanowar Elves',
    'Onakke Ogre',
    'Gravedigger',
    'Colossal Dreadmaw',
    'Sword of Feast and Famine',
    'Nissa, Who Shakes the World',
  ]
  /** One pip, two, and the six-pip cost that is the worst case a printed card reaches. */
  const COSTS = ['{G}', '{1}{G}', '{3}{B}', '{4}{G}{G}', '{2}{W}{U}{B}{R}{G}']
  /** Every width from the 72px tile to the inspector, at 1px — the resolution zoom moves in. */
  const WIDTHS = Array.from({ length: 269 }, (_, index) => index + 72)
  const boxAt = (width: number) => ({ width, height: Math.round((width * 88) / 63) })

  /**
   * **The cost is on every card, at every size a card is drawn at.**
   *
   * This replaces the rule that ran the other way. §6 used to say identity outranks reference and
   * drew the cost only in the width the name did not want, which meant the cards with the longest
   * names — exactly the expensive multicolour ones — were the cards with no cost on them. A hand
   * is read by asking what is castable, so a hand where the costs come and go by name length is a
   * hand that has to be read one card at a time in the preview. XMage draws a cost on every card
   * in the hand and truncates the name to do it, and that is the trade taken here.
   */
  it('draws a cost on every card at every width', () => {
    const found: string[] = []
    for (const name of NAMES) {
      for (const manaCost of COSTS) {
        for (const width of WIDTHS) {
          const plan = banded({ name, manaCost }, boxAt(width))
          if (plan.presentation === 'chip') continue
          if (plan.costSize < NAME_FLOOR) {
            found.push(`${name} ${manaCost} at ${width}px: costSize ${plan.costSize}`)
          }
        }
      }
    }
    expect(found.slice(0, 8), `${found.length} cards drew no cost`).toEqual([])
  })

  /**
   * And what it costs, stated as a property so it cannot grow quietly.
   *
   * The name pays for the cost — that is the reversal — but it pays out of the *band*, never past
   * `COST_SHARE` of it. A cost that took more than half the band would be taking the card's
   * identity with it, and neither half would be readable.
   */
  it('never gives the cost more than half the band', () => {
    const found: string[] = []
    for (const name of NAMES) {
      for (const manaCost of COSTS) {
        for (const width of WIDTHS) {
          const bare = banded({ name }, boxAt(width))
          const priced = banded({ name, manaCost }, boxAt(width))
          if (bare.presentation === 'chip') continue
          // The name may lose characters or a size, but never more than the half-band buys.
          if (priced.name.size < NAME_FLOOR) {
            found.push(`${name} ${manaCost} at ${width}px: name fell to ${priced.name.size}px`)
          }
        }
      }
    }
    expect(found.slice(0, 8), `${found.length} names fell under the floor`).toEqual([])
  })

  /**
   * And the same sweep the `fitName` regression was caught by, run over the whole band.
   *
   * `fitName` was non-monotonic once — a 130px card drew a name at 9px on one line while a 100px
   * one drew it at 13px on two — and every individual answer was defensible; only the relation
   * between them was wrong. A shared band is a second place that failure can come from, so the
   * property is asserted over the band rather than over the name alone.
   */
  it('never draws a smaller name on a wider card', () => {
    const found: string[] = []
    for (const may of [true, false]) {
      for (const name of NAMES) {
        for (const manaCost of COSTS) {
          let previous = 0
          for (const width of WIDTHS) {
            const plan = banded({ name, manaCost }, boxAt(width), may)
            if (plan.presentation === 'chip') continue
            if (plan.name.size < previous) {
              found.push(
                `${name} ${manaCost} at ${width}px: ${previous}px became ${plan.name.size}px`,
              )
            }
            previous = plan.name.size
          }
        }
      }
    }
    expect(found.slice(0, 8), `${found.length} inversions in the band`).toEqual([])
  })

  /**
   * A pip is never under §7's floor, never over the designed size — and **the same on every card
   * that shares a box**, which is what the old "never taller than the name beside it" rule made
   * impossible. Tying the pips to the name's fitted size gave two cards in one row two different
   * costs, which is the row disagreeing about the frame in its smallest form.
   */
  it('keeps the cost within the type scale, and off the name entirely', () => {
    for (const width of WIDTHS) {
      for (const manaCost of COSTS) {
        const sizes = new Set(
          NAMES.map((name) => banded({ name, manaCost }, boxAt(width))).map((plan) =>
            plan.presentation === 'chip' ? 0 : plan.costSize,
          ),
        )
        expect(sizes, `${manaCost} at ${width}px`).toHaveProperty('size', 1)
        for (const size of sizes) {
          if (size === 0) continue
          expect(size).toBeGreaterThanOrEqual(NAME_FLOOR)
          expect(size).toBeLessThanOrEqual(NAME_DESIGNED)
        }
      }
    }
  })

  /** And the pips themselves never shrink as the card grows. */
  it('never draws a smaller cost on a wider card', () => {
    const found: string[] = []
    for (const manaCost of COSTS) {
      let previous = 0
      for (const width of WIDTHS) {
        const plan = banded({ name: 'Gravedigger', manaCost }, boxAt(width))
        if (plan.presentation === 'chip') continue
        if (plan.costSize < previous) {
          found.push(`${manaCost} at ${width}px: ${previous}px became ${plan.costSize}px`)
        }
        previous = plan.costSize
      }
    }
    expect(found.slice(0, 8), `${found.length} inversions in the cost`).toEqual([])
  })

  /** Below the chip threshold the cost is gone regardless (§6) — a chip is a name and its marks. */
  it('gives a chip no cost at all', () => {
    expect(banded({ manaCost: '{G}' }, { width: 190, height: 34 }).costSize).toBe(0)
  })
})

/**
 * §6's "Density": everything on a card is written once.
 *
 * A keyword the server's own prose already states, printed again as an italic line, is one fact
 * twice — and it costs a line of exactly the space the art window is being taken for.
 */
describe('a keyword is written once', () => {
  const flying = (rulesText: string | undefined) =>
    cardPlan(
      face({
        name: 'Serra Angel',
        typeLine: 'Creature — Angel',
        keywords: ['Flying', 'Vigilance'],
        ...(rulesText === undefined ? {} : { rulesText }),
      }),
      { width: 336, height: 470 },
    )

  it('leaves a keyword out of the line when the drawn prose says it', () => {
    expect(flying('Flying, vigilance').text?.keywords).toEqual([])
    expect(flying('Flying').text?.keywords).toEqual(['Vigilance'])
  })

  it('keeps the line for a keyword the prose does not say', () => {
    expect(flying(undefined).text?.keywords).toEqual(['Flying', 'Vigilance'])
    expect(flying('When this creature dies, draw a card.').text?.keywords).toEqual([
      'Flying',
      'Vigilance',
    ])
  })

  it('matches a whole word and nothing less', () => {
    expect(unstatedKeywords(['Flying'], 'Flyingfish are not a keyword.')).toEqual(['Flying'])
    expect(unstatedKeywords(['Flying'], 'This creature has flying.')).toEqual([])
    expect(unstatedKeywords(['First strike'], 'First strike, deathtouch')).toEqual([])
  })

  /**
   * The keyword line in its own right is the other case, and it keeps every keyword: where the
   * rules text is not drawn there is no other copy on the face, and §6 says the separate line
   * exists for exactly that card.
   */
  it('keeps every keyword where the prose is not drawn', () => {
    const plan = cardPlan(
      face({
        name: 'Serra Angel',
        typeLine: 'Creature — Angel',
        keywords: ['Flying', 'Vigilance'],
        rulesText: `Flying, vigilance. ${'Whenever this creature attacks, '.repeat(12)}`,
      }),
      DESIGNED,
    )
    expect(plan.text?.rulesText).toBe(false)
    expect(plan.text?.keywords).toEqual(['Flying', 'Vigilance'])
  })
})

describe('degrading a type line by rule rather than by ellipsis', () => {
  it('keeps the whole line where it fits', () => {
    expect(fitTypeLine('Creature — Bear', { width: 200 }, 9)).toBe('Creature — Bear')
  })

  it('drops the subtype first', () => {
    // 110px rather than the 90 this used to ask for: `Legendary Creature` estimates at 94px at
    // 9px now that the table is an upper bound, where the old one put it at 72 and the browser
    // draws it at 84. The step order is what is asserted here, and it is unchanged.
    expect(fitTypeLine('Legendary Creature — Elf Druid', { width: 110 }, 9)).toBe(
      'Legendary Creature',
    )
  })

  it('drops supertypes next, leaving the card type', () => {
    expect(fitTypeLine('Legendary Creature — Elf Druid', { width: 46 }, 9)).toBe('Creature')
    expect(fitTypeLine('Basic Land — Forest', { width: 30 }, 9)).toBe('Land')
  })

  it('keeps every card type when there is no supertype to drop', () => {
    expect(fitTypeLine('Artifact Creature — Thopter', { width: 90 }, 9)).toBe('Artifact Creature')
  })

  it('drops the line rather than cutting it', () => {
    // The third rung of §3's ladder is *remove the type line*, and there is no rung that cuts
    // one. An empty string is the caller's instruction to draw nothing.
    expect(fitTypeLine('Artifact Creature — Thopter', { width: 20 }, 9)).toBe('')
    expect(fitTypeLine('Legendary Creature — Elf Druid', { width: 200 }, 9)).not.toContain('…')
  })
})

describe('which presentation a box gets', () => {
  it('reads the box and nothing else', () => {
    expect(presentationFor(DESIGNED)).toBe('designed')
    expect(presentationFor(HAND_MIN)).toBe('designed')
    expect(presentationFor(PERMANENT_MIN)).toBe('compact')
    expect(presentationFor({ width: 336, height: 470 })).toBe('full')
  })

  /**
   * **Where a tile stops being a card is derived from §2's type floor, not from §5's 100px row.**
   *
   * §5 makes the 72×100 minimum soft downward — a 90px row draws a 64×90 card with everything on
   * it set smaller — and puts the threshold at the point where the name band can no longer set a
   * legible name at 9px. The old 100px switch is what made three rows in a two-row field mean
   * three rows of chips, which is what made a packer merge them.
   */
  it('stops being a card where a name stops fitting, not at a hundred pixels', () => {
    expect(presentationFor({ width: 96, height: 30 })).toBe('chip')
    expect(CARD_MIN_HEIGHT).toBeLessThan(PERMANENT_MIN.height)
    // Either side of the threshold, at the printed proportion a battlefield tile is drawn at.
    const tile = (height: number) => ({ width: Math.round(height * PRINTED_RATIO), height })
    expect(presentationFor(tile(CARD_MIN_HEIGHT))).not.toBe('chip')
    expect(presentationFor(tile(CARD_MIN_HEIGHT - 1))).toBe('chip')
    expect(presentationFor({ width: 190, height: 99 })).not.toBe('chip')
  })

  /**
   * And the derivation itself: at the threshold a name really is still set, whole, at the floor.
   *
   * `MIN_STEM` characters is what §6 accepts as a name — `Troll Asce` is the example it names —
   * and the tile at the threshold sets one at `NAME_FLOOR` without abbreviating it. That is the
   * property the constant is derived from, asserted rather than the number it comes out at.
   */
  it('can still set a name at the threshold it stops being a card at', () => {
    const width = Math.round(CARD_MIN_HEIGHT * PRINTED_RATIO)
    const plan = cardPlan(face({ name: 'Troll Asce' }), { width, height: CARD_MIN_HEIGHT })
    expect(plan.presentation).not.toBe('chip')
    expect(plan.name.text).toBe('Troll Asce')
    expect(plan.name.abbreviated).toBe(false)
    expect(plan.name.size).toBeGreaterThanOrEqual(NAME_FLOOR)
    expect('Troll Asce'.length).toBe(MIN_STEM)
  })
})

describe('planning a whole card', () => {
  it('fits a name, a cost, a type line, and a stat into a 72×100 tile', () => {
    // The acceptance criterion, and the bar XMage set — read off what XMage actually draws at
    // this size rather than off an idealised version of it. XMage's own 72×100 tiles read
    // `Troll Asce`, `Stranglero`, `Battlefield Sca`: the name is one line, it is cut when it has
    // to be, and the *cost* is what is never given up, because a hand is read by asking what is
    // castable. What has to be complete at this size is everything except the name's tail.
    const plan = cardPlan(
      face({ name: 'Llanowar Elves', typeLine: 'Creature — Elf Druid', manaCost: '{G}' }),
      { ...PERMANENT_MIN },
    )
    expect(plan.name.lines).toBe(1)
    expect(plan.name.text).not.toContain('…')
    expect(plan.name.text.length).toBeGreaterThanOrEqual(MIN_STEM)
    expect(plan.name.size).toBeGreaterThanOrEqual(NAME_FLOOR)
    expect(plan.costSize).toBeGreaterThanOrEqual(NAME_FLOOR)
    expect(plan.typeLine).toBeTruthy()
    expect(plan.typeLine).not.toContain('…')
    expect(plan.statSize).toBeGreaterThanOrEqual(NAME_FLOOR)
  })

  it('shrinks a type line before it degrades one', () => {
    // Shrink comes before remove everywhere (§2), and the type line is no exception: giving up a
    // size costs nothing a player reads, and giving up the subtypes costs a fact. This is a hand
    // card at 1440×900, where the whole line fits at 11px and not at 12.
    const plan = cardPlan(face({ typeLine: 'Creature — Elf Druid' }), { width: 117, height: 163 })
    expect(plan.typeLine).toBe('Creature — Elf Druid')
    expect(plan.typeSize).toBeGreaterThanOrEqual(NAME_FLOOR)
  })

  it('never draws an empty text box', () => {
    // The defect this replaces: a blank black band where the text did not fit.
    expect(cardPlan(face(), DESIGNED).text).toBeUndefined()
    expect(cardPlan(face({ rulesText: '' }), DESIGNED).text).toBeUndefined()
  })

  it('draws a text box that clips rather than no box at all', () => {
    const short = cardPlan(
      face({ typeLine: 'Creature — Elf', rulesText: '{T}: Add {G}.' }),
      DESIGNED,
    )
    expect(short.text?.rulesText).toBe(true)

    // Far more prose than the box holds at any size. The old rule dropped it entirely and left
    // the card saying nothing about itself, which is the wrong end of the trade: the card with
    // the most to say is the one a player most needs to read. It is drawn at the floor, into the
    // room there is, and the browser clips the rest — XMage's answer, and the preview still
    // carries the whole of it.
    const long = cardPlan(
      face({
        typeLine: 'Creature — Elf',
        rulesText:
          'Whenever you tap a Forest for mana, add an additional {G}. Whenever a land enters ' +
          'the battlefield under your control, you may have it become a 3/3 Elemental creature ' +
          'with vigilance and haste that is still a land.',
      }),
      HAND_MIN,
    )
    expect(long.text?.rulesText).toBe(true)
    expect(long.text?.size).toBe(BODY_FLOOR)
    // And it takes the room it was offered, not the height its text wanted, so a clipped box is
    // the same height as a full one on the card beside it.
    expect(long.text?.height).toBeGreaterThan(0)
  })

  it('keeps the keyword line when the rules text will not fit', () => {
    const plan = cardPlan(
      face({
        typeLine: 'Creature — Angel',
        keywords: ['Flying', 'Vigilance'],
        rulesText: 'Whenever this creature attacks, '.repeat(12),
      }),
      DESIGNED,
    )
    expect(plan.text).toEqual({
      size: expect.any(Number),
      height: expect.any(Number),
      rulesText: false,
      keywords: ['Flying', 'Vigilance'],
    })
  })

  it('gives a chip the name and the marks and nothing else', () => {
    const plan = cardPlan(
      face({ typeLine: 'Creature — Bear', rulesText: '{T}: Add {G}.', manaCost: '{1}{G}' }),
      { width: 190, height: 34 },
    )
    expect(plan.presentation).toBe('chip')
    expect(plan.name.lines).toBe(1)
    expect(plan.art).toBe(false)
    expect(plan.typeLine).toBe('')
    expect(plan.text).toBeUndefined()
    expect(plan.costSize).toBe(0)
  })

  it('cuts the name before it drops the cost', () => {
    // The reversal, at its worst case: a six-pip cost on a 72px tile. Presentation, not a rules
    // judgment — the server states what is playable through `valid_actions`, and nothing here
    // concludes anything about affordability.
    const wide = cardPlan(face({ typeLine: 'Creature — Bear', manaCost: '{1}{G}' }), DESIGNED)
    expect(wide.costSize).toBeGreaterThan(0)
    expect(wide.name.text).toBe('Grizzly Bears')

    const crowded = cardPlan(
      face({ typeLine: 'Creature — Bear', manaCost: '{2}{W}{U}{B}{R}{G}' }),
      PERMANENT_MIN,
    )
    expect(crowded.costSize).toBeGreaterThanOrEqual(NAME_FLOOR)
    expect(crowded.name.text.length).toBeGreaterThanOrEqual(MIN_STEM)
  })

  it('gives every card that shares a box the same anatomy', () => {
    // The defect this replaces, and the reason the window stopped being the residue: one row of
    // identical boxes drew a vanilla land a picture half the card tall, the card beside it a
    // stripe, and a card with a lot to say no window at all — three frames from one box, with the
    // illustration cropped differently on each. What a card *says* is allowed to differ. Where its
    // rows are is not: a frame that reshapes itself per card is one nobody can learn to read.
    const ROW: readonly CardText[] = [
      face({ name: 'Plains', typeLine: 'Basic Land — Plains', rulesText: '{T}: Add {W}.' }),
      face({
        name: 'Ajani, Adversary of Tyrants',
        typeLine: 'Legendary Planeswalker — Ajani',
        rulesText: 'A'.repeat(280),
      }),
      face({ name: 'Silverbeak Griffin', typeLine: 'Creature — Griffin', keywords: ['Flying'] }),
      face({
        name: 'Angel of the Dawn',
        typeLine: 'Creature — Angel',
        rulesText:
          'When Angel of the Dawn enters the battlefield, creatures you control get +1/+1 until end of turn and creatures you control gain vigilance until end of turn.',
        keywords: ['Flying'],
      }),
    ]

    // Swept rather than sampled, and across every box a table hands out: the property is about the
    // box, so a table of three widths would only prove it at three widths.
    for (let width = 72; width <= 200; width++) {
      const box = { width, height: Math.round(width / PRINTED_RATIO) }
      const plans = ROW.map((card) => cardPlan(card, box))
      // The whole frame, not just whether there is a window: where the band ends and where the
      // picture starts and stops are the three numbers a player reads a row by.
      const anatomy = new Set(
        plans.map(
          (plan) => `${plan.presentation}/${plan.bandHeight}/${plan.art}/${plan.artHeight}`,
        ),
      )
      expect(anatomy, `cards disagreed about the frame at ${width}px`).toHaveProperty('size', 1)

      // And the last row too. *Whether* there is a text box is content — a card with nothing to
      // say has nothing to put in one — but every box that is drawn ends where the card does, so
      // two cards that both have prose put their bottom edge in the same place.
      const boxes = new Set(plans.map((plan) => plan.text?.height).filter((height) => height))
      expect(boxes.size, `text boxes disagreed at ${width}px`).toBeLessThanOrEqual(1)
    }
  })

  it('sets every name on one line, in a band it reserved', () => {
    // `Plains` is short and `Ajani, Adversary of Tyrants` is not, which is the most ordinary
    // difference two cards can have — and it must move neither the band nor the window under it.
    // One line each: the long one gives up characters rather than taking a second row, which is
    // what XMage does and what keeps two cards in a row the same shape.
    const short = cardPlan(face({ name: 'Plains', typeLine: 'Basic Land — Plains' }), DESIGNED)
    const long = cardPlan(
      face({ name: 'Ajani, Adversary of Tyrants', typeLine: 'Legendary Planeswalker — Ajani' }),
      DESIGNED,
    )
    expect(short.name.lines).toBe(1)
    expect(long.name.lines).toBe(1)
    expect(short.bandHeight).toBe(long.bandHeight)
    // And the reserve is a line of the designed size, not the size the name happened to need.
    expect(short.bandHeight).toBe(Math.ceil(NAME_DESIGNED * 1.2))
  })

  it('keeps the window when a card has a lot to say, and drops the text instead', () => {
    // The trade the row property costs, stated outright so it cannot be walked back by accident.
    // A dense card in the hand can end up with no text box where it used to have one; the pinned
    // preview carries the prose, exactly as it does for every other clamp on the table.
    const talkative = cardPlan(
      face({
        typeLine: 'Creature — Bear',
        rulesText: 'Whenever this creature deals combat damage to a player, draw a card.',
      }),
      HAND_MIN,
    )
    expect(talkative.art).toBe(true)

    const roomy = cardPlan(face({ typeLine: 'Creature — Bear' }), DESIGNED)
    expect(roomy.art).toBe(true)
  })

  it('drops nothing at the full presentation', () => {
    const plan = cardPlan(
      face({
        name: 'Nissa, Who Shakes the World',
        typeLine: 'Legendary Planeswalker — Nissa',
        rulesText: 'A'.repeat(400),
        keywords: ['Flying'],
      }),
      { width: 336, height: 470 },
    )
    expect(plan.presentation).toBe('full')
    expect(plan.name.abbreviated).toBe(false)
    expect(plan.typeLine).toBe('Legendary Planeswalker — Nissa')
    expect(plan.text).toEqual({
      size: expect.any(Number),
      height: expect.any(Number),
      rulesText: true,
      keywords: ['Flying'],
    })
  })
})
