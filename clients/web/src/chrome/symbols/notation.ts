/**
 * The **symbol-notation vocabulary** (issue #462): one tokenizer that turns the
 * server's `{…}` strings — `mana_cost: "{1}{G}"`, `rules_text: "{T}: Add {G}."`,
 * an ability's `label`, a stack `description` — into drawable tokens, plus the
 * plain-text substitution a pure-text context (an `aria-label`) needs.
 *
 * There is exactly **one** vocabulary: {@link tokenizeNotation} feeds both the
 * inline DOM symbols ({@link ../symbols.SymbolText}) and the card face's cost
 * pips (`card/cardFactory.parseManaCost` delegates here), so the two can never
 * drift apart the way they did before this module existed.
 *
 * Rules this module obeys:
 *
 * - **No game logic** (`AGENTS.md`). This is display formatting of a string the
 *   server already composed: nothing here decides what a permanent can produce,
 *   what a cost totals, or whether anything is payable. A code is recognized or
 *   it is not, and an unrecognized one survives as its own literal text — it is
 *   never dropped, guessed at, or crashed on.
 * - **No official symbols** (`docs/brief.md` Legal, ADR 0031). A drawn symbol is
 *   the shipped `PIP` swatch carrying its own letter/number — the same original
 *   treatment the card face's cost row already uses — or, for `{T}`, the
 *   original `tap` glyph from the procedural glyph language. Nothing here traces
 *   or imitates a printed mana/tap symbol.
 * - **Every symbol carries a word.** {@link NotationSymbol.name} is the
 *   accessible name a screen reader announces and the substitution a text-only
 *   surface prints, so no meaning lives in the drawn shape alone.
 */
import { PIP } from '../../tokens';
import type { GlyphName } from '../glyphs';

/** A pip swatch: the disc fill and the glyph color drawn on it. */
export interface SymbolSwatch {
  /** Disc fill color (a `PIP` token). */
  readonly bg: string;
  /** Symbol color drawn on the disc (a `PIP` token). */
  readonly fg: string;
}

/** A recognized symbol, ready to draw and to announce. */
export interface NotationSymbol {
  readonly kind: 'symbol';
  /** The source run including its braces, e.g. `"{G}"`. */
  readonly raw: string;
  /** The code inside the braces, e.g. `"G"`, `"12"`, `"W/U"`. */
  readonly code: string;
  /** What the pip draws — the code itself; the vocabulary adds no ornament. */
  readonly caption: string;
  /** The accessible name / text substitution, e.g. `"green mana"`, `"tap"`. */
  readonly name: string;
  /** The swatch the pip wears. */
  readonly swatch: SymbolSwatch;
  /** An original glyph drawn instead of the caption (only `{T}` today). */
  readonly glyph?: GlyphName;
}

/** One piece of a tokenized string. */
export type NotationToken =
  /** Literal text between symbols, verbatim. */
  | { readonly kind: 'text'; readonly text: string }
  | NotationSymbol
  /** A `{…}` run the vocabulary does not know: rendered as its literal text. */
  | { readonly kind: 'unknown'; readonly raw: string; readonly code: string };

/** The five colors' words, used to build every colored symbol's spoken name. */
const COLOR_WORD: Record<string, string> = {
  W: 'white',
  U: 'blue',
  B: 'black',
  R: 'red',
  G: 'green',
};

/** Number words for the small generic costs that actually appear on cards. */
const GENERIC_WORD = [
  'zero',
  'one',
  'two',
  'three',
  'four',
  'five',
  'six',
  'seven',
  'eight',
  'nine',
  'ten',
];

/** One half of a symbol (or the whole of a simple one): its word and swatch. */
interface SymbolPart {
  /** The spoken word, e.g. `"green"`, `"two generic"`. */
  readonly word: string;
  /** The swatch this part would wear on its own. */
  readonly swatch: SymbolSwatch;
  /** Whether the part is one of the five colors (it wins a hybrid's swatch). */
  readonly colored: boolean;
}

/** Describe one code part, or `null` when the vocabulary does not know it. */
function describePart(part: string): SymbolPart | null {
  const color = COLOR_WORD[part];
  if (color !== undefined) {
    return { word: color, swatch: PIP[part as keyof typeof PIP], colored: true };
  }
  if (part === 'C') return { word: 'colorless', swatch: PIP.N, colored: false };
  if (/^\d+$/.test(part)) {
    const n = Number(part);
    return { word: `${GENERIC_WORD[n] ?? part} generic`, swatch: PIP.N, colored: false };
  }
  if (/^[XYZ]$/.test(part)) return { word: `${part} generic`, swatch: PIP.N, colored: false };
  return null;
}

/**
 * Classify one brace code. Recognized shapes: the tap symbol, a single color /
 * colorless / generic / variable code, a hybrid `A/B`, and a phyrexian `A/P`.
 * Anything else returns `null` and survives as literal text.
 */
function describeCode(code: string): Omit<NotationSymbol, 'kind' | 'raw' | 'code'> | null {
  const key = code.toUpperCase();
  if (key === 'T') {
    return { caption: code, name: 'tap', swatch: PIP.N, glyph: 'tap' };
  }
  const parts = key.split('/');
  if (parts.length === 1) {
    const only = describePart(parts[0]!);
    return only === null ? null : { caption: code, name: `${only.word} mana`, swatch: only.swatch };
  }
  if (parts.length !== 2) return null;
  const [first, second] = parts as [string, string];
  const left = describePart(first);
  if (left === null) return null;
  if (second === 'P') {
    return { caption: code, name: `phyrexian ${left.word} mana`, swatch: left.swatch };
  }
  const right = describePart(second);
  if (right === null) return null;
  return {
    caption: code,
    name: `${left.word} or ${right.word} mana`,
    // A hybrid wears its colored half's swatch (its left one when both are
    // colored), so `{2/W}` reads as white rather than as an anonymous disc.
    swatch: left.colored ? left.swatch : right.colored ? right.swatch : PIP.N,
  };
}

/** Every `{…}` run in a string, with the literal text between them. */
const RUN = /\{([^{}]*)\}/g;

/**
 * Split a server string into literal text and symbol runs, in source order.
 *
 * Total and pure: every character of the input appears in exactly one token, so
 * a caller can rebuild the original string from the tokens and nothing can
 * silently vanish. Adjacent symbols (`{1}{G}`) produce adjacent symbol tokens
 * with no empty text between them.
 */
export function tokenizeNotation(text: string): NotationToken[] {
  const tokens: NotationToken[] = [];
  let at = 0;
  RUN.lastIndex = 0;
  let match = RUN.exec(text);
  while (match !== null) {
    if (match.index > at) tokens.push({ kind: 'text', text: text.slice(at, match.index) });
    const raw = match[0];
    const code = match[1] ?? '';
    const described = describeCode(code);
    tokens.push(
      described === null
        ? { kind: 'unknown', raw, code }
        : { kind: 'symbol', raw, code, ...described },
    );
    at = match.index + raw.length;
    match = RUN.exec(text);
  }
  if (at < text.length) tokens.push({ kind: 'text', text: text.slice(at) });
  return tokens;
}

/** Whether a string carries at least one symbol this vocabulary draws. */
export function hasSymbolNotation(text: string): boolean {
  return tokenizeNotation(text).some((token) => token.kind === 'symbol');
}

/**
 * The **text substitution** (issue #462 AC 2): the same string with every
 * recognized symbol replaced by its spoken name, for the pure-text contexts
 * that cannot hold markup — an `aria-label`, a `title`, an announced hint.
 * `"{T}: Add {G}."` becomes `"tap: Add green mana."`; an unrecognized run stays
 * exactly as the server wrote it.
 *
 * Adjacent symbols — a cost, where the source has no separator to inherit — are
 * separated by a space, so `"{1}{G}"` is spoken as two names rather than run
 * together into one word.
 */
export function symbolNotationText(text: string): string {
  const tokens = tokenizeNotation(text);
  return tokens
    .map((token, i) => {
      if (token.kind === 'text') return token.text;
      const spoken = token.kind === 'symbol' ? token.name : token.raw;
      return tokens[i - 1]?.kind === 'text' || i === 0 ? spoken : ` ${spoken}`;
    })
    .join('');
}
