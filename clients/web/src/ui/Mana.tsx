/**
 * Mana, drawn as pips.
 *
 * One component for every cost on the screen — a card in hand, an object on the stack, a
 * catalog entry in the builder, and the floating mana in front of a seat — over the tokens
 * `mana.ts` produced. A cost is the densest thing on a card and the thing a player scans for
 * first, so it is the one place where a row of shapes beats a row of characters.
 *
 * The shapes are the project's own: a flat disc with a letter in it. Nothing here reproduces an
 * official symbol, and nothing is downloaded — this is CSS and a glyph from the page's own font.
 *
 * **Pips are decoration to assistive technology.** A circle with a `G` in it says nothing when
 * read out, so the row is one labelled image and the label is `spokenCost` — which is why a card
 * frame must never put the pips somewhere the label would be its only text.
 */
import type { CSSProperties } from 'react'

import { manaSymbols, spokenCost, type ManaSymbol } from './../mana'

/**
 * The colours a pip is filled with, as custom properties the stylesheet resolves.
 *
 * The values themselves stay in CSS. What is decided here is only *which* of them this pip
 * uses, so a hybrid's two halves and a colour's one fill come out of the same rule and the
 * palette remains a single place to change.
 */
function fill(symbol: ManaSymbol): CSSProperties | undefined {
  const [first, second] = symbol.colors
  if (!first) return undefined
  return {
    '--pip-from': `var(--mana-${first})`,
    '--pip-to': `var(--mana-${second ?? first})`,
    // The ink follows the first half, which is the half a split pip is read from.
    '--pip-ink': `var(--mana-${first}-ink)`,
  } as CSSProperties
}

export function ManaPip({ symbol }: { symbol: ManaSymbol }) {
  return (
    <span
      className={`mana__pip mana__pip--${symbol.kind}${symbol.colors.length > 1 ? ' mana__pip--split' : ''}`}
      style={fill(symbol)}
    >
      {/* The slash is what separates the halves in text; the pip separates them by being split
          in two, so repeating it inside a disc this small only costs legibility. */}
      {symbol.glyph.replace(/\//g, '')}
    </span>
  )
}

/**
 * One printed cost as a row of pips.
 *
 * Renders nothing at all for a card the server sent no cost for. An empty row would read as
 * "this costs nothing", which is a different claim from "no cost was sent" — a land, a token,
 * and an ability on the stack are all the second one.
 */
export function ManaCost({ cost, className }: { cost?: string; className?: string }) {
  const symbols = manaSymbols(cost)
  if (symbols.length === 0) return null
  return (
    <span
      className={['mana', className].filter(Boolean).join(' ')}
      role="img"
      aria-label={spokenCost(cost)}
    >
      {symbols.map((symbol, index) => (
        <ManaPip key={index} symbol={symbol} />
      ))}
    </span>
  )
}
