/**
 * The tint class a face is washed in: the palette `cards.css` holds, named.
 *
 * Shared because the wash belongs to the *face*, not to whoever is drawing it — a card, or the
 * title bar alone. **Go through here rather than composing `card-${tint}`**: two of the names do
 * not match their tint (`multicolor` wears the gold palette, `colorless` the grey one), so a
 * hand-built class silently resolves to no palette, which draws as a black box.
 */
import type { CardFace } from './../../card-face'
import { frameTint } from './../../mana'

const TINT: Record<string, string> = {
  w: 'card-w',
  u: 'card-u',
  b: 'card-b',
  r: 'card-r',
  g: 'card-g',
  multicolor: 'card-gold',
  colorless: 'card-c',
}

/**
 * The class for a face — its printed pips, and the colour identity the server stated, which is
 * what a land has in place of a cost.
 */
export const tintClass = (face: CardFace): string =>
  TINT[frameTint(face.manaCost, face.colorIdentity)] ?? 'card-c'
