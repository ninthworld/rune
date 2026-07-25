/**
 * Inline symbol rendering (issue #462): the one DOM surface component that
 * draws a server string's `{…}` notation as icons and everything else as plain
 * text.
 *
 * Every DOM surface that shows server prose — the inspector's cost and rules,
 * an ability control's label, the prompt strip, a stack entry, a card face's
 * rules area — renders it through here, so brace notation reaches the player
 * nowhere and the drawn vocabulary is defined in exactly one place
 * ({@link tokenizeNotation}).
 *
 * The drawn symbol is Rune's own: a `PIP`-swatched disc carrying the code's own
 * letter or number — the same original treatment the card face's cost row uses,
 * at the same scale relative to its text — or, for `{T}`, the procedural `tap`
 * glyph. No official symbol is imitated (`docs/brief.md` Legal, ADR 0031).
 *
 * Accessibility (§AC 2/3): each symbol is a `role="img"` with the vocabulary's
 * spoken name, so a screen reader announces "green mana" rather than a letter;
 * a code the vocabulary does not know renders as its literal text rather than
 * disappearing.
 */
import { Fragment, type CSSProperties } from 'react';
import { cx } from '../cx';
import { Glyph } from '../glyphs';
import { tokenizeNotation, type NotationSymbol } from './notation';
import s from './symbols.module.css';

/** Props for {@link SymbolText}. */
export interface SymbolTextProps {
  /** The server string, rendered verbatim apart from its `{…}` runs. */
  text: string;
  /** Extra class on each drawn symbol (never on the surrounding text). */
  symbolClassName?: string;
}

/** One drawn symbol: the swatch disc, and either a glyph or the code itself. */
function Symbol({ token, className }: { token: NotationSymbol; className?: string }) {
  return (
    <span
      className={cx(s.symbol, className)}
      // Colors ride the shipped `PIP` card tokens as custom properties (ADR
      // 0019) — the stylesheet holds no literal.
      style={
        {
          '--symbol-bg': token.swatch.bg,
          '--symbol-fg': token.swatch.fg,
        } as CSSProperties
      }
      data-symbol={token.code}
      // A multi-part code (`W/U`, `2/W`) needs a wider plate than a disc; the
      // attribute is the stylesheet's hook, and it is also the non-color
      // channel that separates a hybrid from a plain pip.
      data-wide={token.glyph === undefined && token.caption.length > 1 ? 'true' : undefined}
      role="img"
      aria-label={token.name}
    >
      {token.glyph === undefined ? token.caption : <Glyph name={token.glyph} />}
    </span>
  );
}

/**
 * Render `text` with its symbol notation drawn as icons. Returns a fragment, so
 * the caller keeps ownership of the surrounding element and its styling.
 */
export function SymbolText({ text, symbolClassName }: SymbolTextProps) {
  return (
    <>
      {tokenizeNotation(text).map((token, i) => {
        if (token.kind === 'symbol') {
          return <Symbol key={i} token={token} className={symbolClassName} />;
        }
        // Text and unrecognized runs alike print exactly what the server sent.
        return <Fragment key={i}>{token.kind === 'text' ? token.text : token.raw}</Fragment>;
      })}
    </>
  );
}
