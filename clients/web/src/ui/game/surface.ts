/**
 * What every table surface needs in order to draw a card, and nothing more.
 *
 * These three are derived once by `Game.tsx` from the one view it was handed, then passed down.
 * A surface receives the answers rather than the view, which is what keeps a surface from
 * quietly growing a second reading of the game: there is no `valid_actions` in a `Battlefield`
 * to be tempted by, only a function that already decided.
 */
import type { CardFace, CardFaceState } from './../../card-face'

export interface Surface {
  /** How this face is taking part in the interaction the server advertised. */
  stateOf(face: CardFace): CardFaceState
  /** Open the inspector. Never a game action. */
  inspect(face: CardFace): void
  /** The display name for an entity id the view mentioned — a card, a permanent, a seat. */
  labelFor(id: string): string
}
