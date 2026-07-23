/**
 * Client-to-server in-game messages: action choices and priority stops.
 */

import type { EntityId } from './index.js';
import type { Phase } from './view.js';

/**
 * The player's answer to one choice slot — a {@link TargetRequirement} or a
 * {@link Prompt} — keyed back to the slot by `slot`. The same shape answers every
 * slot kind; each id must be one of that slot's advertised candidates / options /
 * items or the server treats the action as a no-op.
 */
export interface TargetChoice {
  /** The {@link TargetRequirement} or {@link Prompt} slot this answers. */
  slot: string;
  /**
   * The entity ids chosen for this slot: a target id; the chosen
   * {@link PromptOption.id} for an option; the selected ids for a select-from-zone;
   * or the full ordering for an order prompt.
   */
  chosen?: EntityId[];
}

/**
 * The client's chosen action, answered atomically: the `id` of one issued
 * `valid_actions` entry, its content-binding {@link ChooseAction.token} echoed
 * verbatim, and one {@link ChooseAction.targets} entry per requirement slot.
 * Serializes with a `type` discriminator (`{"type":"choose_action", ...}`),
 * matching the `ClientMessage::ChooseAction` variant. `token` and `targets` are
 * omitted when empty, so a plain action's message is just the id.
 */
export interface ChooseAction {
  /** Discriminator for the client→server message envelope. */
  type: 'choose_action';
  /** The `id` of the chosen {@link ValidAction}. */
  action_id: string;
  /** The chosen action's {@link ValidAction.token}, echoed verbatim (or omitted). */
  token?: string;
  /** One entry per {@link ValidAction.requirements} slot; omitted when empty. */
  targets?: TargetChoice[];
}

/**
 * Build the client→server envelope for choosing an issued action. `token` (the
 * chosen action's content-binding token) and `targets` (the assembled per-slot
 * selection) are included only when present, so a plain action stays a bare id —
 * this is pure envelope assembly, never legality or effect computation.
 */
export function chooseAction(
  actionId: string,
  token?: string,
  targets?: TargetChoice[],
): ChooseAction {
  const message: ChooseAction = { type: 'choose_action', action_id: actionId };
  if (token !== undefined && token !== '') message.token = token;
  if (targets !== undefined && targets.length > 0) message.targets = targets;
  return message;
}

/**
 * Set (or replace) this connection's **priority-stop preferences** (issue #264): the
 * steps at which the seat wants priority even when it has no meaningful action, so
 * basic auto-pass does not skip it there. Serializes with the same tagged envelope as
 * {@link ChooseAction} (`{"type":"set_stops", ...}`), matching the
 * `ClientMessage::SetStops` variant. The set replaces the seat's current one wholesale;
 * an empty set (omitted `stops`) clears all stops. Server-authoritative and
 * reconnect-durable — the server stores it per seat and reflects it in
 * {@link GameView.stops}; the client computes no legality here.
 */
export interface SetStopsMessage {
  /** Discriminator for the client→server message envelope. */
  type: 'set_stops';
  /** The steps to stop at; omitted when clearing all stops. */
  stops?: Phase[];
}

/** Build a `set_stops` message, eliding `stops` when the set is empty (clear all). */
export function setStopsMessage(stops: Phase[]): SetStopsMessage {
  const message: SetStopsMessage = { type: 'set_stops' };
  if (stops.length > 0) message.stops = stops;
  return message;
}
