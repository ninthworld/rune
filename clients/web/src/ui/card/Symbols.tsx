/**
 * Server prose with its symbols drawn.
 *
 * The server writes `Pay {G}` and `{T}: Add {G}.`; a player reads a pip. The wording is the
 * server's byte for byte and only the parts between braces change (`mana.ts`) — which is what
 * lets an action's label, a slot's prompt and a card's rules text all be shown as written
 * without any of them being composed here.
 */
import { inlineSymbols, spokenSymbol } from './../../mana'
import { Pip } from './Pips'

export function Symbols({ text }: { text: string }) {
  return (
    <>
      {inlineSymbols(text).map((token, index) =>
        token.kind === 'symbol' ? (
          <Pip key={index} symbol={token.symbol.glyph} label={spokenSymbol(token.symbol)} inline />
        ) : (
          <span key={index}>{token.text}</span>
        ),
      )}
    </>
  )
}
