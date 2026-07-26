/**
 * The direct-manipulation half of the action contract (issue #554): where an action
 * may be *taken to*, and the acknowledgement that closes the loop on a submission.
 *
 * Both exist for the same reason the rest of `valid_actions` does — so the client
 * computes nothing. {@link ActionDestination} answers "where may I drop this?" with
 * server-authoritative surfaces instead of a client-side table of which card type
 * belongs in which zone; {@link ActionAck} answers "did my click land?" by naming the
 * exact submission a view responds to, which {@link GameView.action_rejected} alone
 * never could.
 */

import type { PlayerId } from './index.js';

/**
 * One server-authoritative **destination** an action may be taken to: the drop
 * regions a direct-manipulation gesture is allowed to offer.
 *
 * The client derives its drop regions from **exactly** this list and **fails
 * closed**: an action with no `destinations` has no drop target at all, and an entry
 * whose {@link ActionDestination.type} it does not recognize is ignored rather than
 * guessed at. A client that decided "a land is dropped on the battlefield, a spell on
 * the stack" would be encoding rules, and would be wrong the first time a card said
 * otherwise.
 *
 * Drag stays **optional input**: every action reachable by a drop is also reachable
 * by clicking the action, by keyboard, and by touch. Destinations say where a drag
 * *may* release, never how an action must be taken.
 */
export interface ActionDestination {
  /**
   * What this destination names — `"zone"`, `"entity"`, or `"player"` today. A
   * free-form string, not a union, for the same reason {@link ValidAction.type} is:
   * new kinds must not break older clients, which ignore what they do not recognize
   * and therefore offer no drop region for it.
   */
  type: string;
  /**
   * The destination itself: a zone name (`"battlefield"`, `"stack"`, `"command"`) for
   * a `zone`, an entity id for an `entity`, or a {@link PlayerId} for a `player`.
   * Opaque — matched against the surfaces the client already renders, never parsed.
   */
  id: string;
  /**
   * Whose copy of a per-player zone this is (a graveyard, a command zone). Absent for
   * a shared zone such as the battlefield or the stack, and for entity/player
   * destinations, which name their own subject.
   */
  owner?: PlayerId;
  /**
   * Human-readable label for the drop region, when the server has something more
   * useful to say than the surface's own name. Absent otherwise, in which case the
   * client labels the region however it already labels that surface.
   */
  label?: string;
}

/**
 * The server's **acknowledgement** of one submitted action, carried on the
 * {@link GameView} that answers it.
 *
 * Before this, a client sent a {@link ChooseAction} and watched for *some* view to
 * arrive; it could not tell that view apart from a broadcast another seat's action
 * caused, so a pending indicator either cleared on the wrong message or had to be
 * timed out. And {@link GameView.action_rejected}, the only feedback there was, says
 * *that* something was rejected, never *which* submission.
 *
 * The correlation id is the client's own: it puts an opaque
 * {@link ChooseAction.submission} on the message and the server echoes it back here
 * verbatim, never parsing or deriving it. A view answering no submission carries no
 * ack at all, so the ack's presence is itself the signal. Transient and advisory —
 * the UI reconstructs fully without it.
 */
export interface ActionAck {
  /** The {@link ChooseAction.submission} this view answers, echoed verbatim. */
  submission: string;
  /**
   * Whether the server **applied** the submitted action. `false` means it was
   * rejected and the game is unchanged — the same event
   * {@link GameView.action_rejected} flags, now tied to a specific submission.
   * Always present, so a client never has to read an absence as a verdict.
   */
  accepted: boolean;
}
