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
  const colors = new Set<ManaColor>()
  for (const symbol of manaSymbols(cost)) {
    for (const color of symbol.colors) colors.add(color)
  }
  if (colors.size > 1) return 'multicolor'
  for (const color of colors) return color
  return 'colorless'
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

export function spokenCost(cost: string | undefined): string {
  const symbols = manaSymbols(cost)
  if (symbols.length === 0) return ''
  const words = symbols.map((symbol) => SPOKEN[symbol.kind]?.(symbol) ?? symbol.glyph)
  return `${words.join(' ')} mana`
}
