/**
 * What every table surface needs in order to draw a card, and nothing more.
 *
 * These three are derived once by `Game.tsx` from the one view it was handed, then passed down.
 * A surface receives the answers rather than the view, which is what keeps a surface from
 * quietly growing a second reading of the game: there is no `valid_actions` in a `Battlefield`
 * to be tempted by, only a function that already decided.
 *
 * `activate` is the same reason in the other direction. A surface reports *that an object was
 * clicked* and learns nothing about what happened next; whether the click filled a target slot,
 * selected a subject, or opened the inspector is one rule, in `interaction.ts`, applied the same
 * way from the hand, the board, a pile, and the stack.
 */
import type { CardFaceState } from './../../card-face'

export interface Surface {
  /**
   * How this object is taking part in the interaction the server advertised.
   *
   * Keyed by entity id rather than by face, because the objects a player acts on are not all
   * cards: a seat is a target of half the burn in the format, and asking it the same question
   * as a permanent is what keeps the two answerable by the same click.
   */
  stateOf(id: string): CardFaceState
  /** This object was clicked. What that means is decided centrally, never here. */
  activate(id: string): void
  /** The display name for an entity id the view mentioned — a card, a permanent, a seat. */
  labelFor(id: string): string
}
