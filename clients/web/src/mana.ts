/**
 * A printed mana cost, as symbols something can draw.
 *
 * `mana_cost` arrives as one string — `{2}{G}{G}`, `{G/U}`, `{X}{R}` — because that is how the
 * card is printed. A row of pips is the form a player actually reads it in, so the string is
 * tokenized here, once, and every surface that shows a cost renders the same tokens: the hand,
 * the stack, the inspector, the deck builder, and the floating mana in front of a seat.
 *
 * **This reads a string; it decides nothing about the game.** A symbol's `kind` is what the
 * braces contain and nothing else. Whether a cost can be paid, what it counts as, and what a
 * hybrid pip may be paid with are rules questions, and the server answers all of them by either
 * listing an action or not listing it. Ask this module whether `{2/W}` is affordable and there
 * is nothing here to answer with.
 *
 * The same applies to `costTint`, which exists so a board is scannable by colour and is
 * deliberately the shallowest possible reading: the colours of the pips that were printed. It is
 * not colour (CR 105), not colour identity (CR 903.4), and not devotion. A card with a colour
 * indicator, a colour-setting ability, or no cost at all tints neutral — which is the honest
 * answer for a client that was sent a cost string and nothing else, and is why the tint is only
 * ever a background wash under text that says the real thing.
 *
 * Unknown contents survive as themselves. A symbol this build has never seen renders as its own
 * letters in a plain pip rather than being dropped, so a newer server's card is legible here
 * before this client knows what it is.
 */

/** The five colours, lowercased so they can key a CSS custom property directly. */
export type ManaColor = 'w' | 'u' | 'b' | 'r' | 'g'

const COLORS: Record<string, ManaColor | undefined> = { W: 'w', U: 'u', B: 'b', R: 'r', G: 'g' }

/** What the braces contained — a shape to draw, never a rules classification. */
export type ManaSymbolKind =
  | 'generic'
  | 'variable'
  | 'color'
  | 'hybrid'
  | 'phyrexian'
  | 'colorless'
  | 'snow'
  | 'tap'
  | 'untap'
  | 'other'

export interface ManaSymbol {
  /** The symbol exactly as printed, braces included, so it can be shown as text anywhere. */
  printed: string
  /** What the pip draws inside itself. */
  glyph: string
  kind: ManaSymbolKind
  /**
   * The colours this one pip is drawn in, in printed order. Empty for anything that is not
   * coloured — generic, `{C}`, `{S}`, `{T}`, or a symbol this build does not recognise.
   */
  colors: readonly ManaColor[]
}

/**
 * Split a printed cost into its symbols.
 *
 * Tolerant on purpose: text outside braces is kept as one `other` symbol rather than discarded,
 * because dropping it would silently shorten a cost, and a cost that is quietly wrong is worse
 * than one that looks unfamiliar.
 */
export function manaSymbols(cost: string | undefined): readonly ManaSymbol[] {
  if (!cost) return []
  const symbols: ManaSymbol[] = []
  for (const match of cost.matchAll(/\{([^}]*)\}|([^{}]+)/g)) {
    const [printed, braced, loose] = match
    const inner = braced ?? loose ?? ''
    if (inner.trim() === '') continue
    symbols.push({ printed, ...classify(inner.trim()) })
  }
  return symbols
}

function classify(inner: string): Omit<ManaSymbol, 'printed'> {
  const glyph = inner.toUpperCase()

  if (/^\d+$/.test(glyph)) return { glyph, kind: 'generic', colors: [] }

  // A composite pip: `{G/U}` either colour, `{2/W}` a colour or two generic, `{G/P}` two life.
  // Every part that names a colour is a colour this pip is drawn in; which of them is *paid* is
  // the server's business, and the pip shows all of them because that is what is printed.
  if (glyph.includes('/')) {
    const parts = glyph.split('/')
    const colors = parts.flatMap((part) => {
      const color = COLORS[part]
      return color ? [color] : []
    })
    return { glyph, kind: parts.includes('P') ? 'phyrexian' : 'hybrid', colors }
  }

  const color = COLORS[glyph]
  if (color) return { glyph, kind: 'color', colors: [color] }

  switch (glyph) {
    case 'X':
    case 'Y':
    case 'Z':
      return { glyph, kind: 'variable', colors: [] }
    case 'C':
      return { glyph, kind: 'colorless', colors: [] }
    case 'S':
      return { glyph, kind: 'snow', colors: [] }
    case 'T':
      return { glyph, kind: 'tap', colors: [] }
    case 'Q':
      return { glyph, kind: 'untap', colors: [] }
    default:
      return { glyph, kind: 'other', colors: [] }
  }
}

/**
 * The wash a frame is drawn in.
 *
 * One of the five, `multicolor` for a cost printing more than one, or `colorless` for everything
 * else — which covers a land, an artifact, a card whose colour comes from somewhere this client
 * cannot see, and a card with no cost at all. See the module note: this is the colours of the
 * printed pips, and it is not any of the rules concepts it resembles.
 */
export type FrameTint = ManaColor | 'multicolor' | 'colorless'

export function costTint(cost: string | undefined): FrameTint {
  return tintOf(costColors(cost))
}

/** The colours the printed pips of `cost` are drawn in, deduplicated. */
function costColors(cost: string | undefined): ReadonlySet<ManaColor> {
  const colors = new Set<ManaColor>()
  for (const symbol of manaSymbols(cost)) {
    for (const color of symbol.colors) colors.add(color)
  }
  return colors
}

const tintOf = (colors: ReadonlySet<ManaColor>): FrameTint => {
  if (colors.size > 1) return 'multicolor'
  for (const color of colors) return color
  return 'colorless'
}

/** The wire's colour letters, lowercased into the keys the palette is written in. */
const WIRE_COLORS: Record<string, ManaColor | undefined> = {
  W: 'w',
  U: 'u',
  B: 'b',
  R: 'r',
  G: 'g',
}

/**
 * The wash a frame is drawn in, given everything the server said about the card.
 *
 * The printed pips come first, because that is what a player sees on the card itself and it is
 * the answer for almost every card that has a cost. **Colour identity is the fallback**, and it
 * exists for the cards the cost cannot answer for: a Forest costs nothing, prints no coloured
 * pip, and used to draw in the same grey as a Wastes and an artifact — which made a mana base,
 * the thing a player counts most often, the least scannable part of the board.
 *
 * The order matters and is not arbitrary. A card whose cost prints colours is *tinted by its
 * cost*, so a card with an off-colour activated ability does not change colour in a hand; only
 * a card the cost says nothing about falls through to what the server says it belongs to. Both
 * halves are server-stated (`docs/protocol.md`); neither is a rules judgment made here, and
 * neither is the card's colour (CR 105), which this still deliberately does not claim to be.
 */
export function frameTint(
  cost: string | undefined,
  colorIdentity: readonly string[] = [],
): FrameTint {
  const printed = costColors(cost)
  if (printed.size > 0) return tintOf(printed)
  const identity = new Set<ManaColor>()
  for (const letter of colorIdentity) {
    const color = WIRE_COLORS[letter]
    if (color) identity.add(color)
  }
  return tintOf(identity)
}

/**
 * A cost read aloud, for the text a screen reader gets instead of the pips.
 *
 * The pips themselves are decoration to assistive technology — a circle with a letter in it says
 * nothing — so the frame hides them and exposes this. Deliberately close to how a player says
 * it out loud rather than to the wire string.
 */
const SPOKEN: Partial<Record<ManaSymbolKind, (symbol: ManaSymbol) => string>> = {
  generic: (symbol) => symbol.glyph,
  variable: (symbol) => symbol.glyph,
  color: (symbol) => spokenParts(symbol),
  hybrid: (symbol) => `${spokenParts(symbol)} hybrid`,
  // The `P` is the marker that made this Phyrexian, so it is said once as the word rather than
  // twice as a letter nobody pronounces.
  phyrexian: (symbol) => `${spokenParts(symbol, 'P')} Phyrexian`,
  colorless: () => 'colorless',
  snow: () => 'snow',
  tap: () => 'tap',
  untap: () => 'untap',
}

const COLOR_NAMES: Record<ManaColor, string> = {
  w: 'white',
  u: 'blue',
  b: 'black',
  r: 'red',
  g: 'green',
}

const spokenParts = (symbol: ManaSymbol, ...drop: string[]): string =>
  symbol.glyph
    .split('/')
    .filter((part) => !drop.includes(part))
    .map((part) => {
      const color = COLORS[part]
      return color ? COLOR_NAMES[color] : part
    })
    .join(' or ')

/** One symbol, said aloud. `{T}` in a sentence is "tap", not a picture of a tap. */
export const spokenSymbol = (symbol: ManaSymbol): string =>
  SPOKEN[symbol.kind]?.(symbol) ?? symbol.glyph

export function spokenCost(cost: string | undefined): string {
  const symbols = manaSymbols(cost)
  if (symbols.length === 0) return ''
  return `${symbols.map(spokenSymbol).join(' ')} mana`
}

// ---------------------------------------------------------------------------
// Symbols inside a sentence
// ---------------------------------------------------------------------------

/** A run of prose, or one symbol standing in the middle of it. */
export type InlineToken = { kind: 'text'; text: string } | { kind: 'symbol'; symbol: ManaSymbol }

/**
 * Split server-generated rules text into prose and symbols.
 *
 * `"{T}: Add {G}."` is how the wire writes a sentence a player reads as a tap symbol, a colon,
 * and a green pip. Leaving the braces on screen is the difference between a card and a debug
 * dump — and the pips are already drawn everywhere else, so a rules box spelling them out is the
 * one place the same symbol looks like something else.
 *
 * Deliberately the opposite tolerance from `manaSymbols`. That one is reading a *cost*, where
 * anything outside braces is a malformed cost worth keeping as a symbol; this is reading a
 * *sentence*, where everything outside braces is the sentence.
 */
export function inlineSymbols(text: string): readonly InlineToken[] {
  const tokens: InlineToken[] = []
  for (const match of text.matchAll(/\{([^}]*)\}|([^{]+)/g)) {
    const [, braced, prose] = match
    if (prose !== undefined) {
      tokens.push({ kind: 'text', text: prose })
    } else if (braced !== undefined) {
      const inner = braced.trim()
      // An empty pair of braces is not a symbol. Keeping it as prose means a server that emits
      // one shows something odd rather than an empty disc nobody can explain.
      if (inner === '') tokens.push({ kind: 'text', text: match[0] })
      else tokens.push({ kind: 'symbol', symbol: { printed: match[0], ...classify(inner) } })
    }
  }
  return tokens
}
