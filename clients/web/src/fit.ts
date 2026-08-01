/**
 * What a card has room to say, given the box it was handed.
 *
 * SAGE ships no card art, so the printed name is the *only* thing that identifies a card
 * (`docs/client-design.md` §6). Anything that competes with it for horizontal space competes
 * with the card's identity — which is how a hand of `C…`, `Dis…`, `Sna…` happened: the cost took
 * a fixed slice of a 100px band and the name got what was left.
 *
 * The rule this module implements is §2's: **type size, then line count, then completeness — and
 * truncation is not a step in it.** That is the order things are *sacrificed* in, not a sequence
 * of loops: wrapping is one of the ways text fits at a given size, so the answer is the largest
 * size at which the text fits within the lines the box can afford, fewer lines preferred at equal
 * size. Only where the floor cannot hold it — and only on the battlefield, never in the hand
 * where a player is choosing — is a name allowed to give up characters, and then only down to a
 * stem that still names a card a player knows. `Troll Asce` is one. `C…` is not information
 * at all.
 *
 * XMage is the bar: a complete name, cost, type line, keyword line, and P/T inside 72×100, all
 * of it complete, at roughly 9px. Complete-and-small is readable; large-and-truncated is not.
 *
 * **Pure, and deliberately so.** No DOM, no React, no measurement — a component hands it a box
 * and gets back a plan, so the whole fitting policy is testable without a browser. The width of
 * a string is *estimated* from a small per-character table rather than measured, which is
 * accurate enough for a decision and biased on purpose: every ratio below is rounded up, so the
 * answer is that text is wider than it really is. Erring that way costs a font size or a line;
 * erring the other way would clip, and CSS `overflow` is the backstop rather than the plan.
 *
 * **Nothing here is a rules judgment.** Dropping a mana cost from a small tile is a drawing
 * decision about a string the server sent, and degrading `Legendary Creature — Elf Druid` to
 * `Creature` is typography over a display line — never a claim about what the card *is*, which
 * only the server's `card_types` states (`board.ts`).
 */
import { manaSymbols } from './mana'

/** A box to draw in, in CSS pixels. Where it comes from is the surface's business, not this. */
export interface Box {
  width: number
  height: number
}

/** One fitted string, and what it cost to make it fit. */
export interface Fitted {
  /** What to draw. Equal to the input unless `abbreviated`. */
  text: string
  lines: 1 | 2
  /** Effective px, never below the floor it was given. */
  size: number
  /** Characters were given up. Visual only — assistive technology still gets the whole name. */
  abbreviated: boolean
}

/** The type scale's card floors and designed sizes (`docs/client-design.md` §7). */
export const NAME_FLOOR = 9
export const NAME_DESIGNED = 13
export const BODY_FLOOR = 9
export const BODY_DESIGNED = 12
export const STAT_FLOOR = 9
export const STAT_DESIGNED = 14

/**
 * The shortest abbreviation that is still a card.
 *
 * Ten characters, which is what `Troll Asce` is — the example §6 names as recognisable. Below
 * this an abbreviation stops being a shorter name and becomes a prefix, and a prefix identifies
 * nothing on a board with two cards that share it. If a name cannot be fitted at this length the
 * box was sized wrong; the stem is drawn anyway rather than shrinking further, because the fix
 * belongs to whatever chose the box.
 */
export const MIN_STEM = 10

// ---------------------------------------------------------------------------
// Estimating a string's width
// ---------------------------------------------------------------------------

/**
 * Advance widths as a fraction of the font size, in four buckets.
 *
 * Calibrated against the spec's own arithmetic — §6 expects roughly 13 characters a line from a
 * 9px name in a 72px tile, which is about 0.58em average — and rounded up from there. The name
 * band is drawn semibold and the type line and rules are not, so one table for both over-states
 * the lighter of the two, which is the safe direction.
 */
const NARROW = new Set(" ijltfrI.,:;'!|()[]{}/-".split(''))
const WIDE = new Set('mwMW@'.split(''))

function advance(character: string): number {
  // The long dash a type line is printed with is a full em, and it is the one character that
  // would be badly mis-estimated by a default: every creature's type line contains one.
  if (character === '—') return 1
  if (NARROW.has(character)) return 0.32
  if (WIDE.has(character)) return 0.92
  // Capitals and digits carry more ink than lowercase, and a name is full of both.
  if (character >= 'A' && character <= 'Z') return 0.68
  if (character >= '0' && character <= '9') return 0.6
  return 0.55
}

/** How wide a run of text would be drawn, in px. Over-estimated on purpose; see above. */
export function textWidth(text: string, size: number): number {
  let em = 0
  for (const character of text) em += advance(character)
  return em * size
}

/**
 * Greedy word wrap, the way a browser does it.
 *
 * A word wider than the whole line is kept on a line of its own rather than broken: a card name
 * long enough for that is a sizing defect (§6), and reporting one over-long line lets the caller
 * see that rather than hiding it behind a hyphenation this does not model.
 */
export function wrapText(text: string, width: number, size: number): readonly string[] {
  const lines: string[] = []
  let current = ''
  for (const word of text.split(/\s+/).filter(Boolean)) {
    const candidate = current ? `${current} ${word}` : word
    if (current && textWidth(candidate, size) > width) {
      lines.push(current)
      current = word
    } else {
      current = candidate
    }
  }
  if (current) lines.push(current)
  return lines
}

/** Whether the text lays out inside `lines` lines of `width`, with no line running over. */
function wraps(text: string, width: number, size: number, lines: number): boolean {
  const laid = wrapText(text, width, size)
  return laid.length <= lines && laid.every((line) => textWidth(line, size) <= width)
}

// ---------------------------------------------------------------------------
// The name
// ---------------------------------------------------------------------------

export interface NameOptions {
  floor: number
  designed: number
  /**
   * Whether the third step of the ladder is available at all.
   *
   * False in the hand and in the deck builder — the two places a player *chooses* from what
   * they are reading, where §6 forbids abbreviation outright. Not a presentation tier and not a
   * variant: it does not change what the card is, it removes one way of degrading it.
   */
  mayAbbreviate: boolean
  /** A landscape chip has one line and no more. Two everywhere else. */
  maxLines?: 1 | 2
}

/**
 * A card's name, fitted: the largest size that works, then — last and only where it is allowed —
 * abbreviated.
 *
 * **One search over size, not two.** Size is what §2 sacrifices first, so the outer loop walks it
 * down from the designed size; but the line count is one of the ways a string *fits* at a size
 * rather than a step that happens after shrinking, so every line count the box can afford is
 * tried before the size is given up, fewer lines first because a name on one line is read in one
 * movement. Reading it the other way — all the way to the 9px floor on one line, and only then a
 * second line — makes the result non-monotonic in the box: a 130px card drew
 * `Sword of Feast and Famine` at 9px on one line while a 100px card drew it at 13px on two. The
 * widest card there is rendering the smallest text is indefensible, and it is why §2 now says so
 * outright.
 */
export function fitName(name: string, box: { width: number }, opts: NameOptions): Fitted {
  const width = Math.max(0, box.width)
  const maxLines = opts.maxLines ?? 2
  const floor = opts.floor
  const top = Math.max(floor, opts.designed)

  const counts: readonly (1 | 2)[] = maxLines === 1 ? [1] : [1, 2]
  for (let size = top; size >= floor; size--) {
    for (const lines of counts) {
      if (wraps(name, width, size, lines)) return { text: name, lines, size, abbreviated: false }
    }
  }

  // The floor, reached. Where abbreviating is forbidden the whole name is returned anyway: §6 is
  // explicit that a name that will not fit at the floor is a box sized wrong, and drawing the
  // card's identity is worth more than honouring a box that was already wrong.
  if (!opts.mayAbbreviate || name.length <= MIN_STEM) {
    return { text: name, lines: maxLines, size: floor, abbreviated: false }
  }
  return abbreviate(name, width, floor, maxLines)
}

/** Trailing space and the punctuation a cut leaves dangling — `Nissa,` reads worse than `Nissa`. */
const trimStem = (stem: string): string => stem.replace(/[\s,;:.'\-—]+$/u, '')

function abbreviate(name: string, width: number, size: number, maxLines: number): Fitted {
  const lines = maxLines === 1 ? 1 : 2
  for (let end = name.length - 1; end >= MIN_STEM; end--) {
    const stem = trimStem(name.slice(0, end))
    if (stem.length < MIN_STEM) break
    if (wraps(stem, width, size, lines)) {
      return { text: stem, lines: maxLines === 1 ? 1 : 2, size, abbreviated: true }
    }
  }
  // No stem of the minimum length fits. Draw the minimum stem regardless — see `MIN_STEM`.
  return {
    text: trimStem(name.slice(0, MIN_STEM)) || name.slice(0, MIN_STEM),
    lines: maxLines === 1 ? 1 : 2,
    size,
    abbreviated: true,
  }
}

// ---------------------------------------------------------------------------
// The type line
// ---------------------------------------------------------------------------

/**
 * The supertypes, as printed words.
 *
 * A closed list of five adjectives, used to drop a word from a display string and for nothing
 * else. It decides nothing about the card: what a card *is* comes from the server's
 * `card_types`, which is the whole reason `board.ts` never reads a type line either. An
 * unrecognised leading word is kept, so a newer server's supertype degrades to the full line
 * rather than to something wrong.
 */
const SUPERTYPES = new Set(['Basic', 'Legendary', 'Ongoing', 'Snow', 'World'])

/** Everything printed before the long dash — the supertypes and card types, without subtypes. */
const beforeDash = (typeLine: string): string => typeLine.split(/[—–]/u)[0]!.trim()

const withoutSupertypes = (typeLine: string): string => {
  const words = typeLine.split(/\s+/).filter(Boolean)
  let start = 0
  while (start < words.length - 1 && SUPERTYPES.has(words[start]!)) start += 1
  return words.slice(start).join(' ')
}

/**
 * A type line, degraded **by rule rather than by ellipsis**.
 *
 * `Legendary Creature — Elf Druid` becomes `Legendary Creature`, then `Creature`, and never
 * `Legendary Cr…`. A whole word says what a card is; a cut one says nothing and looks like a
 * defect. When even the card types will not fit the answer is the empty string — the type line
 * is the third thing off the card in §3's ladder, and dropping it is a stated step where cutting
 * it is not.
 */
export function fitTypeLine(typeLine: string, box: { width: number }, size: number): string {
  for (const step of degradations(typeLine)) {
    if (textWidth(step, size) <= box.width) return step
  }
  return ''
}

/** The line, then the line without its subtypes, then without its supertypes. Never empty steps. */
function degradations(typeLine: string): readonly string[] {
  const trimmed = typeLine.trim()
  const stripped = beforeDash(trimmed)
  const steps = [trimmed, stripped, withoutSupertypes(stripped)].filter(Boolean)
  return [...new Set(steps)]
}

/**
 * The type line and the size to draw it at, together.
 *
 * Shrinking comes before removing, here as everywhere (§2), so the whole line at 10px is
 * preferred to `Creature` at 12px: giving up a size costs nothing a player reads, and giving up
 * the subtypes costs a fact. Only when no size down to the floor holds a step does the next
 * degradation get a turn.
 */
function fitTypeBand(
  typeLine: string,
  box: { width: number },
  top: number,
): { text: string; size: number } {
  for (const step of degradations(typeLine)) {
    for (let size = top; size >= BODY_FLOOR; size--) {
      if (textWidth(step, size) <= box.width) return { text: step, size }
    }
  }
  return { text: '', size: BODY_FLOOR }
}

// ---------------------------------------------------------------------------
// The whole card
// ---------------------------------------------------------------------------

/**
 * The four presentations of §6. Not scaled copies of one another: each drops what it has no room
 * for, in the ladder's order.
 */
export type Presentation = 'full' | 'designed' | 'compact' | 'chip'

/** Below this height a tile is no longer a card but a landscape chip (§3, step 5). */
const CARD_MIN_HEIGHT = 100
/**
 * Wide enough that nothing has to be dropped: the inspector, and the pointer's preview over the
 * side column. Set at the narrower of the two, because the preview is the surface that redeems
 * everything the table clamped and it is the column's width, not a chosen one.
 */
const FULL_WIDTH = 200
/** The hand's minimum width (§5), and the width below which a text box is not worth attempting. */
const COMPACT_WIDTH = 100

/**
 * Which presentation a box gets.
 *
 * **A function of the box and nothing else** — a battlefield card and a hand card of the same
 * size are the same card, so no surface names its own tier. Height decides only whether a tile
 * is still a card, which is §4's rule and the one thing there that can be evaluated rather than
 * judged; width decides how much of a card it is.
 */
export function presentationFor(box: Box): Presentation {
  if (box.height < CARD_MIN_HEIGHT) return 'chip'
  if (box.width >= FULL_WIDTH) return 'full'
  if (box.width < COMPACT_WIDTH) return 'compact'
  return 'designed'
}

/** Everything on a face that competes for room. A subset of `CardFace`, so this stays pure. */
export interface CardText {
  name: string
  manaCost?: string
  typeLine?: string
  rulesText?: string
  keywords: readonly string[]
}

/** What to draw, and how big. Anything omitted is one gesture away in the preview or inspector. */
export interface CardPlan {
  presentation: Presentation
  name: Fitted
  /** The cost, over the art's top-right corner. `0` when there is no room for it. */
  costSize: number
  /** Whether an art window is drawn at all. It takes what is left, and may be left nothing. */
  art: boolean
  /** The type line as it will be drawn — `''` when it did not fit and is dropped. */
  typeLine: string
  typeSize: number
  /** The text box, or nothing at all. **An empty one is never drawn.** */
  text: { size: number; rulesText: boolean; keywords: boolean } | undefined
  statSize: number
}

/** Near zero, because §6's density is padding spent on text. */
const PAD = 2
/** The frame's own 1px rule. Inside `box-sizing: border-box` it comes out of the text's width. */
const BORDER = 1
const GAP = 2
/** Below this an art window is a stripe, and a stripe is worth less than the room it takes. */
const MIN_ART = 20
/** The text box's own inset. */
const TEXT_PAD = 3
/** A pip is about this many times its font size wide, gap included (`cards.css`). */
const PIP_ADVANCE = 1.3
/** Pips are graphic rather than text, but a disc smaller than this states nothing either. */
const MIN_PIP = 7

const lineHeight = (size: number): number => Math.ceil(size * 1.2)

/**
 * How much room the corner stat needs beside the name — it is drawn over the frame's own edge,
 * so only a chip, which is one row, has to leave a hole for it.
 */
const statSizeFor = (box: Box): number =>
  Math.round(Math.min(STAT_DESIGNED, Math.max(STAT_FLOOR, box.width * 0.11)))

export interface PlanOptions {
  /** See `NameOptions.mayAbbreviate`. False in the hand and the deck builder. */
  mayAbbreviate?: boolean
}

/**
 * The whole card, planned against its box.
 *
 * The order is §2's contract read top down: the name is fitted first and out of the full width,
 * because it is the one fact that cannot be inferred; then the type line, then the text box, each
 * only if it fits complete at its floor; and **the art window takes what is left**, which may be
 * nothing. Art is the only element that can degrade to zero without costing a fact, which is
 * exactly why it is last in the queue rather than holding a fixed share.
 */
export function cardPlan(content: CardText, box: Box, opts: PlanOptions = {}): CardPlan {
  const presentation = presentationFor(box)
  const statSize = statSizeFor(box)
  const mayAbbreviate = opts.mayAbbreviate ?? true
  const inner = {
    width: Math.max(0, box.width - 2 * (PAD + BORDER)),
    height: Math.max(0, box.height - 2 * (PAD + BORDER)),
  }

  // A chip is one landscape row: the name, and the marks beside it. No art, no cost, no prose —
  // below the card floor those are not small versions of themselves, they are noise. The stat
  // shares the row here rather than sitting over a corner, so the name is fitted without it.
  if (presentation === 'chip') {
    return {
      presentation,
      name: fitName(
        content.name,
        { width: Math.max(0, inner.width - statSize * 2.4) },
        { floor: NAME_FLOOR, designed: NAME_DESIGNED, mayAbbreviate, maxLines: 1 },
      ),
      costSize: 0,
      art: false,
      typeLine: '',
      typeSize: 0,
      text: undefined,
      statSize,
    }
  }

  // The inspector and the preview are where everything the table clamps is redeemed, so nothing
  // is fitted against a height there: the panel grows instead.
  if (presentation === 'full') {
    return {
      presentation,
      name: fitName(content.name, inner, {
        floor: NAME_FLOOR,
        designed: 17,
        mayAbbreviate: false,
      }),
      costSize: 13,
      art: true,
      typeLine: content.typeLine?.trim() ?? '',
      typeSize: BODY_DESIGNED,
      text:
        content.rulesText || content.keywords.length > 0
          ? { size: BODY_DESIGNED, rulesText: true, keywords: true }
          : undefined,
      statSize: STAT_DESIGNED,
    }
  }

  return fitTable(content, presentation, inner, statSize, mayAbbreviate)
}

/** The two presentations drawn on the table, where height is the thing that runs out. */
function fitTable(
  content: CardText,
  presentation: 'designed' | 'compact',
  inner: Box,
  statSize: number,
  mayAbbreviate: boolean,
): CardPlan {
  const name = fitName(content.name, inner, {
    floor: NAME_FLOOR,
    designed: NAME_DESIGNED,
    mayAbbreviate,
  })
  let left = inner.height - lineHeight(name.size) * name.lines - GAP

  // The type line never outgrows the name above it, and never the body's designed size.
  const top = Math.round(
    Math.min(BODY_DESIGNED, Math.max(BODY_FLOOR, Math.min(name.size, inner.width * 0.11))),
  )
  let typeLine = ''
  let typeSize = top
  if (content.typeLine && left >= lineHeight(top) + GAP) {
    const band = fitTypeBand(content.typeLine, inner, top)
    typeLine = band.text
    typeSize = band.size
    if (typeLine) left -= lineHeight(typeSize) + GAP
  }

  // §6's table names the *order* things leave in, not a fixed manifest: rules text is what
  // `compact` gives up, and everything below the name is drawn while it fits. The keyword line
  // survives at both, because it is one of the five things XMage fits onto the 72×100 tile this
  // whole section is measured against.
  const text = fitTextBox(content, inner, left, name.size, presentation === 'designed')
  if (text) left -= text.height + GAP

  return {
    presentation,
    name,
    costSize: costSizeFor(content.manaCost, inner.width, name.size),
    art: left >= MIN_ART,
    typeLine,
    typeSize,
    text: text && { size: text.size, rulesText: text.rulesText, keywords: text.keywords },
    statSize,
  }
}

/**
 * The text box, drawn **only when what goes in it fits whole**.
 *
 * A box showing the first three lines of a card's rules is a truncation wearing a border, and §6
 * forbids it: the pointer's preview carries the complete text continuously and costs no click.
 * So the box is offered the rules and the keywords together, then the keywords alone — the
 * keyword line — and then nothing at all. That last case is the defect this replaces: a blank
 * black band where the text did not fit, a container outliving its content.
 */
function fitTextBox(
  content: CardText,
  inner: Box,
  available: number,
  designed: number,
  allowRules: boolean,
): { size: number; height: number; rulesText: boolean; keywords: boolean } | undefined {
  const keywords = content.keywords.length > 0 ? content.keywords.join(' · ') : ''
  const rules = allowRules ? content.rulesText : undefined
  if (available <= GAP) return undefined

  const candidates: { rulesText: boolean; keywords: boolean; parts: string[] }[] = []
  if (rules) candidates.push({ rulesText: true, keywords: !!keywords, parts: [rules, keywords] })
  if (keywords) candidates.push({ rulesText: false, keywords: true, parts: [keywords] })

  const top = Math.max(BODY_FLOOR, Math.min(designed, BODY_DESIGNED))
  for (const candidate of candidates) {
    for (let size = top; size >= BODY_FLOOR; size--) {
      const width = inner.width - 2 * TEXT_PAD
      // A word wider than the line is not one line: `wrapText` leaves it whole and the browser
      // breaks it, so what it costs is counted here rather than being reported as fitting.
      const lines = candidate.parts
        .filter(Boolean)
        .reduce(
          (total, part) =>
            total +
            wrapText(part, width, size).reduce(
              (rows, line) => rows + Math.max(1, Math.ceil(textWidth(line, size) / width)),
              0,
            ),
          0,
        )
      const height = lines * lineHeight(size) + 2 * TEXT_PAD
      if (height <= available - GAP) {
        return { size, height, rulesText: candidate.rulesText, keywords: candidate.keywords }
      }
    }
  }
  return undefined
}

/**
 * The cost, as a row of pips over the art's top-right corner.
 *
 * Out of the name's row entirely, which is the point: pips are graphic and read at sizes text
 * does not, so they can be small where a name cannot. When even a small row would take more than
 * half the width the cost drops — the server already states what is playable through
 * `valid_actions`, so a cost is reference rather than a decision input, and dropping it is a
 * drawing decision that concludes nothing about affordability.
 */
function costSizeFor(manaCost: string | undefined, width: number, nameSize: number): number {
  const pips = manaSymbols(manaCost).length
  if (pips === 0) return 0
  const size = Math.floor(Math.min(nameSize, (width * 0.55) / (pips * PIP_ADVANCE)))
  return size >= MIN_PIP ? size : 0
}
