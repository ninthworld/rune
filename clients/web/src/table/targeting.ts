/**
 * Targeting-mode session logic — a small **pure** state machine over an action's
 * server-supplied `requirements` (ADR 0009 §Client).
 *
 * The web client enters targeting mode entirely from the target requirements the
 * server carries on a {@link ValidAction}: it walks the requirement slots as a
 * prompt queue, offering only the candidate entity ids the server enumerated, and
 * assembles one atomic answer (`token` + one {@link TargetChoice} per slot) that
 * the store submits in a single `ChooseAction`. No legality, cost, or effect is
 * computed here — the session only records which offered candidate the player
 * picked per slot (hard rule: zero game logic in the client).
 *
 * A {@link TargetingSession} is **ephemeral UI state**, structurally the same as
 * the entity selection: it is reconstructable from the current GameView plus the
 * player's in-progress input, and it is discarded whenever a fresh view arrives.
 * Nothing here is load-bearing across messages.
 */
import type { EntityId, TargetChoice, TargetRequirement, ValidAction } from '../protocol';

/** Whether an action needs the player to fill target slots before it can be taken. */
export function requiresTargets(action: ValidAction): boolean {
  return (action.requirements?.length ?? 0) > 0;
}

/**
 * An in-progress targeting selection: the action being answered plus the entity
 * ids picked for each already-completed slot, in requirement order. One inner
 * array per slot (one id per slot in the single-target model this slice ships;
 * the shape already generalizes to multi-select).
 */
export interface TargetingSession {
  /** The multi-step action being resolved (carries `requirements` and `token`). */
  action: ValidAction;
  /** Picks recorded for the already-completed slots, in requirement order. */
  picks: EntityId[][];
}

/** Begin targeting an action. Callers gate this on {@link requiresTargets}. */
export function beginTargeting(action: ValidAction): TargetingSession {
  return { action, picks: [] };
}

/** The action's ordered requirement slots (empty when it has none). */
function requirementsOf(session: TargetingSession): TargetRequirement[] {
  return session.action.requirements ?? [];
}

/**
 * The requirement slot the player is currently filling, or `null` once every slot
 * is filled (the session is ready to submit).
 */
export function activeRequirement(session: TargetingSession): TargetRequirement | null {
  const reqs = requirementsOf(session);
  return session.picks.length < reqs.length ? reqs[session.picks.length] : null;
}

/**
 * The legal candidate entity ids for the active slot — the only things the UI may
 * make targetable. Empty once the session is complete or if the slot listed none.
 */
export function activeCandidates(session: TargetingSession): EntityId[] {
  return activeRequirement(session)?.candidates ?? [];
}

/** Whether every requirement slot has been filled. */
export function isComplete(session: TargetingSession): boolean {
  return activeRequirement(session) === null;
}

/**
 * Record a pick for the active slot, returning the advanced session. The caller
 * only ever offers server-listed candidates, so this appends the choice without
 * re-checking legality. A no-op once the session is already complete.
 */
export function pick(session: TargetingSession, entityId: EntityId): TargetingSession {
  if (isComplete(session)) return session;
  return { action: session.action, picks: [...session.picks, [entityId]] };
}

/**
 * Whether the session has a pick to take back — the gate the UNDO control
 * renders behind. `false` on a session sitting at its first slot: there is
 * nothing to retract there, and CANCEL is what leaves the action entirely.
 */
export function canRetract(session: TargetingSession): boolean {
  return session.picks.length > 0;
}

/**
 * Drop the most recent pick, returning the session reopened at that slot.
 *
 * One step, never the session: the action and every earlier pick survive, so a
 * player who mis-clicks the second target of a three-target spell re-picks it
 * without re-choosing the first. Discarding the whole session is
 * {@link beginTargeting}'s inverse and belongs to CANCEL, not to UNDO — the
 * control's accessible name promises exactly this. A no-op with no picks, so the
 * caller may wire it unconditionally.
 */
export function retract(session: TargetingSession): TargetingSession {
  if (!canRetract(session)) return session;
  return { action: session.action, picks: session.picks.slice(0, -1) };
}

/**
 * Assemble the atomic answer once every slot is filled: one {@link TargetChoice}
 * per requirement, keyed by its `slot`. Returns `null` while the session is still
 * incomplete, so the caller only submits a fully-answered action.
 */
export function assembleTargets(session: TargetingSession): TargetChoice[] | null {
  const reqs = requirementsOf(session);
  if (session.picks.length < reqs.length) return null;
  return reqs.map((req, i) => ({ slot: req.slot, chosen: session.picks[i] }));
}
