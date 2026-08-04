/**
 * What an object's own actions are, and when they belong at the object rather than in the dock.
 *
 * The dock has always drawn this list — select a permanent and it offers exactly what the server
 * attached to it. What this module decides is that the same list is *also* drawn where the player
 * is looking, because the trip to the dock is the whole cost of an object's second action, and a
 * player choosing between two abilities of one creature is looking at the creature.
 *
 * **It is the same list, from the same place, taken by the same path.** Nothing here ranks,
 * filters, or renames an action — `actionsFor` is the whole of the selection rule, and a
 * "primary" action chosen by type would be this client interpreting what an action does.
 *
 * **It is not a context menu.** No new gesture exists: the menu opens on the click that already
 * produced `{kind: 'select'}` in `interaction.ts`, which is the gesture for "this object has more
 * than one action and no single meaning". Right-click keeps meaning inspect — #639's argument for
 * a one-click action is that reading costs nothing and is available whatever else is in progress,
 * and a menu on that gesture would make reading a card cost a menu traversal. It is also the only
 * gesture in the client with no keyboard equivalent, and everything else here is a button.
 */
import type { ValidAction } from './protocol'
import { actionsFor, type Interaction } from './interaction'

/** One object's actions, at the object. */
export interface ObjectMenu {
  /** The entity the menu belongs to — the identifier the server used, never a client key. */
  id: string
  /**
   * Exactly the actions the server attached to that id, in the order the view listed them.
   * May be empty: an object can be selected and have nothing offered for it right now, and
   * saying so beside the object is a better answer than a menu that refuses to open.
   */
  actions: readonly ValidAction[]
}

/**
 * Whether an object's actions are currently being asked about, and which.
 *
 * Five states outrank it, and each for the same reason — something else already owns the
 * player's next click, and a menu floating over the board during any of them is a second thing
 * claiming to be the question:
 *
 * - **A draft is armed.** The action is now asking *which objects*, and the board is the answer
 *   sheet. A menu over a candidate would sit between the question and the card that answers it.
 * - **A card is being paid for.** Saying "I am playing this" selects that card, and the answer
 *   to it is out on the board: the mana sources are lit, the bar carries the cost, and the next
 *   click is meant for a land. The card itself owns nothing to take — that is *why* it went down
 *   this path (`interaction.gestureFor`) — so the menu that used to open over the hand was an
 *   empty panel between the player and the sources they were being asked to tap.
 * - **A confirmation is open.** Conceding is one button asked twice, and every other click is a
 *   "no" — including, deliberately, one that went to a menu instead.
 * - **A submission is in flight.** Nothing can be taken until the server answers, so a list of
 *   things to take is a list of disabled buttons.
 * - **Nothing is selected.** There is no object, and the dock is where actions with no object
 *   live.
 *
 * The payment case ends by itself: the moment the server offers a cast for that card, confirming
 * takes it — and where it offers more than one, `Board` drops the intent and selects the card
 * outright, which is a selection with a real list behind it and opens here as any other does.
 */
export function objectMenu(
  actions: readonly ValidAction[],
  interaction: Interaction,
): ObjectMenu | undefined {
  if (interaction.selected === undefined) return undefined
  if (interaction.armed !== undefined) return undefined
  if (interaction.paying !== undefined) return undefined
  if (interaction.confirming !== undefined) return undefined
  if (interaction.pending !== undefined) return undefined

  return { id: interaction.selected, actions: actionsFor(actions, interaction.selected) }
}
