import type { EntityId, ValidAction } from '../../protocol';
import type { RenderTier } from '../../card/cardFactory';
import type { BandRowKind, SurfaceTier } from './types';

/** The subject-actions from `valid_actions[]` that name a given entity. */
export function actionsFor(entityId: EntityId, actions: ValidAction[]): ValidAction[] {
  return actions.filter((a) => a.subject?.includes(entityId));
}

/** The combat-declaration kinds whose candidates get direct entity entry (ADR 0025). */
const DECLARATION_KINDS = new Set(['declare_attackers', 'declare_blockers']);

/**
 * The single offered subject-less combat declaration listing `entityId` among
 * its requirement candidates (ADR 0025), or `undefined` when none. Only the two
 * combat declarations participate — reversible toggle-and-confirm flows where
 * "click the creature" is unmistakably the player's intent; other multi-select
 * actions (mulligan bottoming, zone selections) keep their explicit entry.
 */
function declarationFor(entityId: EntityId, actions: ValidAction[]): ValidAction | undefined {
  const matches = actions.filter(
    (a) =>
      DECLARATION_KINDS.has(a.type) &&
      (a.subject === undefined || a.subject.length === 0) &&
      (a.requirements ?? []).some((r) => (r.candidates ?? []).includes(entityId)),
  );
  return matches.length === 1 ? matches[0] : undefined;
}

export { declarationFor };

/** The per-row tiers at a surface tier (creatures lead one step over support). */
function tiersForSurface(surface: SurfaceTier): Record<BandRowKind, RenderTier> {
  if (surface === 'field') {
    return { creatures: 'field', support: 'support', lands: 'chip' };
  }
  if (surface === 'support') {
    return { creatures: 'support', support: 'mini', lands: 'chip' };
  }
  return { creatures: 'mini', support: 'mini', lands: 'chip' };
}

export { tiersForSurface };

/** One step down the tier ladder (blueprint §Density ladder rung 2). */
function stepDown(surface: SurfaceTier): SurfaceTier {
  return surface === 'field' ? 'support' : 'mini';
}

export { stepDown };
