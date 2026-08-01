/**
 * Server-generated rules text, with its symbols drawn.
 *
 * `"{T}: Add {G}."` is how the wire writes a sentence a player reads as a tap symbol, a colon,
 * and a green pip. Leaving the braces on screen is the difference between a card and a debug
 * dump — and every cost on the table is already pips, so a rules box spelling them out is the
 * one place the same symbol looks like something else.
 *
 * Nothing is rewritten, reworded, or reflowed: the prose is the server's, byte for byte
 * (ADR 0008 §7), and the only change is that the parts between braces become discs.
 *
 * Each pip is labelled where a cost's are not. A row of pips after a card's name is one thing —
 * a cost — and is read as one; a pip in the middle of a sentence is a word in that sentence, and
 * a screen reader that skipped it would hear "colon Add period".
 */
import { inlineSymbols, spokenSymbol } from './../mana'
import { ManaPip } from './Mana'

export function RulesText({
  text,
  className,
  title,
}: {
  text: string
  className?: string
  /** The full string as a tooltip, for a surface that clamps this one. */
  title?: string
}) {
  return (
    <span className={className} title={title}>
      {inlineSymbols(text).map((token, index) =>
        token.kind === 'text' ? (
          token.text
        ) : (
          <span key={index} role="img" aria-label={` ${spokenSymbol(token.symbol)} `}>
            <ManaPip symbol={token.symbol} />
          </span>
        ),
      )}
    </span>
  )
}
