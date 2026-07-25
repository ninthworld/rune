/**
 * Emphasis policy for the relationship grammar
 * (`docs/design/stack-and-relationships.md` §4.4, implementation note IN6).
 *
 * This is the generalisation of `combatLinks.ts::selectVisibleLinks` to every
 * relationship kind: focus **isolates** one object's relationships, and a
 * crowded board calms the rest. It differs from the shipped combat policy in one
 * deliberate way — a relationship is never *removed*, only reduced. §4.4's
 * `endpoint-only` state is the scalability floor: the corridor stops filling
 * with strokes but both endpoints stay drawn, so the relationship is never
 * silently lost (decision D11).
 *
 * Pure over declared effects. It reads no rects, no geometry, and no game state:
 * every relationship it re-states was already stated by the server.
 */
import type { EntityId } from '../protocol';
import { COMBAT_LINK } from '../tokens';
import type { PersistentEffect } from './effects';

/** Whether an anchor names `id` directly or as that player's seat cluster. */
function anchorNames(anchor: PersistentEffect['from'], id: EntityId): boolean {
  if (!('ref' in anchor)) return false;
  return anchor.ref === id || anchor.ref === `seat:${id}` || anchor.ref === `stack:${id}`;
}

/** Whether a relationship touches `id` at either end. */
export function relationshipTouches(effect: PersistentEffect, id: EntityId): boolean {
  return anchorNames(effect.from, id) || anchorNames(effect.to, id);
}

/**
 * A relationship the player is actively building — a live targeting session's
 * pending or provisional path. It is the answer to a question on screen right
 * now and is never calmed or reduced to endpoints, whatever else the board is
 * doing.
 */
function isLiveInput(effect: PersistentEffect): boolean {
  const state = effect.state ?? (effect.category === 'targeting-path' ? 'pending' : 'confirmed');
  return state === 'pending' || state === 'provisional';
}

/**
 * Apply §4.4's emphasis states to a declared relationship set.
 *
 * - **Attachment brackets and source tethers** are untouched: they are neutral
 *   line work stating structure, not emphasis (§4.3 R9).
 * - **A live targeting session** is untouched (see {@link isLiveInput}).
 * - **With an object isolated** (focused / selected), its own relationships keep
 *   full emphasis and every other confirmed one drops to `calmed`.
 * - **With nothing isolated and more than `crowdedThreshold` confirmed paths**,
 *   they all drop to `endpoint-only` — caps but no stroke, two ops each.
 */
export function applyRelationshipEmphasis(
  effects: readonly PersistentEffect[],
  isolatedId: EntityId | null | undefined,
): PersistentEffect[] {
  const emphasised = effects.filter(
    (effect) =>
      effect.category !== 'attachment-bracket' &&
      effect.category !== 'source-tether' &&
      !isLiveInput(effect),
  );
  const crowded = isolatedId == null && emphasised.length > COMBAT_LINK.crowdedThreshold;
  if (!crowded && isolatedId == null) return [...effects];
  return effects.map((effect) => {
    if (
      effect.category === 'attachment-bracket' ||
      effect.category === 'source-tether' ||
      isLiveInput(effect)
    ) {
      return effect;
    }
    if (isolatedId != null) {
      return relationshipTouches(effect, isolatedId) ? effect : { ...effect, state: 'calmed' };
    }
    return { ...effect, state: 'endpoint-only' };
  });
}
