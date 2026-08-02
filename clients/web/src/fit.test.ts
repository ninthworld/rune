import { describe, expect, it } from 'vitest'

import {
  cardPlan,
  fitName,
  fitTypeLine,
  MIN_STEM,
  NAME_DESIGNED,
  NAME_FLOOR,
  presentationFor,
  textWidth,
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

  it('stops being a card below a hundred pixels of height', () => {
    expect(presentationFor({ width: 190, height: 99 })).toBe('chip')
    expect(presentationFor({ width: 96, height: 30 })).toBe('chip')
  })
})

describe('planning a whole card', () => {
  it('fits a complete name, a type line, and a stat into a 72×100 tile', () => {
    // The acceptance criterion, and the bar XMage set.
    const plan = cardPlan(face({ name: 'Llanowar Elves', typeLine: 'Creature — Elf Druid' }), {
      ...PERMANENT_MIN,
    })
    expect(plan.name.text).toBe('Llanowar Elves')
    expect(plan.name.abbreviated).toBe(false)
    expect(plan.name.size).toBeGreaterThanOrEqual(NAME_FLOOR)
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

  it('draws a text box only when what goes in it fits whole', () => {
    const short = cardPlan(
      face({ typeLine: 'Creature — Elf', rulesText: '{T}: Add {G}.' }),
      DESIGNED,
    )
    expect(short.text?.rulesText).toBe(true)

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
    expect(long.text).toBeUndefined()
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
    expect(plan.text).toEqual({ size: expect.any(Number), rulesText: false, keywords: true })
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

  it('drops the cost before it drops anything the name needs', () => {
    // Presentation, not a rules judgment: the server states what is playable through
    // `valid_actions`, and nothing here concludes anything about affordability.
    const wide = cardPlan(face({ typeLine: 'Creature — Bear', manaCost: '{1}{G}' }), DESIGNED)
    expect(wide.costSize).toBeGreaterThan(0)

    const crowded = cardPlan(
      face({ typeLine: 'Creature — Bear', manaCost: '{2}{W}{U}{B}{R}{G}' }),
      PERMANENT_MIN,
    )
    expect(crowded.costSize).toBe(0)
    expect(crowded.name.text).toBe('Grizzly Bears')
  })

  it('lets the art window take what is left, down to nothing', () => {
    // Art is the one element that degrades to zero without costing a fact, so it queues last —
    // a card with a lot to say spends the window on saying it.
    const roomy = cardPlan(face({ typeLine: 'Creature — Bear' }), { width: 130, height: 182 })
    expect(roomy.art).toBe(true)

    const talkative = cardPlan(
      face({
        typeLine: 'Creature — Bear',
        rulesText: 'Whenever this creature deals combat damage to a player, draw a card.',
      }),
      HAND_MIN,
    )
    expect(talkative.text?.rulesText).toBe(true)
    expect(talkative.art).toBe(false)
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
    expect(plan.text).toEqual({ size: expect.any(Number), rulesText: true, keywords: true })
  })
})
