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
import type { CardFaceLink, CardFaceState } from './../../card-face'

export interface Surface {
  /**
   * How this object is taking part in the interaction the server advertised.
   *
   * Keyed by entity id rather than by face, because the objects a player acts on are not all
   * cards: a seat is a target of half the burn in the format, and asking it the same question
   * as a permanent is what keeps the two answerable by the same click.
   */
  stateOf(id: string): CardFaceState
  /**
   * How this object stands in the relationships of whatever is currently focused.
   *
   * A separate question from `stateOf`, and deliberately so: one is about an action the server
   * offered, the other about a relationship the server projected, and a dense board has both at
   * once. Collapsing them would make "this creature can be targeted" and "this creature blocked
   * the thing you clicked" render as the same emphasis.
   */
  linkOf(id: string): CardFaceLink
  /** This object was clicked. What that means is decided centrally, never here. */
  activate(id: string): void
  /**
   * The player is looking at this object, or has looked away (`undefined`).
   *
   * Purely a look: it submits nothing, selects nothing, and is thrown away the moment the
   * pointer moves. It is what decides which relationships are emphasised, and it is separate
   * from clicking because the objects most worth tracing are often the ones with no action to
   * click — a blocker, an enchanted creature, a spell someone else controls.
   */
  trace(id: string | undefined): void
  /** The display name for an entity id the view mentioned — a card, a permanent, a seat. */
  labelFor(id: string): string
}
