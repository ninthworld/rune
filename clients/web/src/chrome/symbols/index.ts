/**
 * Symbol notation (issue #462): the single `{…}` vocabulary shared by every DOM
 * surface that shows server prose, and by the card face's cost pips.
 *
 * - {@link SymbolText} — draw a string with its symbols as inline icons.
 * - {@link symbolNotationText} — the plain-text substitution for `aria-label`s
 *   and other contexts that cannot hold markup.
 * - {@link tokenizeNotation} — the tokenizer both of the above are built on.
 */
export { SymbolText, type SymbolTextProps } from './SymbolText';
export {
  hasSymbolNotation,
  symbolNotationText,
  tokenizeNotation,
  type NotationSymbol,
  type NotationToken,
  type SymbolSwatch,
} from './notation';
