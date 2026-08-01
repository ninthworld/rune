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
})

describe('fitting a name: shrink, then wrap, then abbreviate', () => {
  it('draws a short name at the designed size on one line', () => {
    expect(name('Forest', 130)).toEqual({
      text: 'Forest',
      lines: 1,
      size: NAME_DESIGNED,
      abbreviated: false,
    })
  })

  it('shrinks before it wraps', () => {
    // Wide enough for `Llanowar Elves` on one line, but not at 13px. Shrinking is the first
    // step, so it stays one line at a smaller size rather than taking a second at a larger one.
    const fitted = name('Llanowar Elves', 78)
    expect(fitted.lines).toBe(1)
    expect(fitted.size).toBeLessThan(NAME_DESIGNED)
    expect(fitted.size).toBeGreaterThanOrEqual(NAME_FLOOR)
    expect(fitted.text).toBe('Llanowar Elves')
  })

  it('wraps to a second line rather than shrinking below the floor', () => {
    // Wide enough for the name at 9px would keep it on one line, so this is the width where
    // shrinking has genuinely run out: the floor cannot hold it and the second line is the
    // next rung rather than the first.
    const fitted = name('Nissa, Who Shakes the World', HAND_MIN.width)
    expect(fitted.lines).toBe(2)
    expect(fitted.size).toBeGreaterThanOrEqual(NAME_FLOOR)
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

describe('degrading a type line by rule rather than by ellipsis', () => {
  it('keeps the whole line where it fits', () => {
    expect(fitTypeLine('Creature — Bear', { width: 200 }, 9)).toBe('Creature — Bear')
  })

  it('drops the subtype first', () => {
    expect(fitTypeLine('Legendary Creature — Elf Druid', { width: 90 }, 9)).toBe(
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
