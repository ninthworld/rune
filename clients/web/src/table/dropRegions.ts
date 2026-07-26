/**
 * Drop regions for a direct-manipulation gesture (issue #554) — derived from the
 * server's `ValidAction.destinations` and from **nothing else**.
 *
 * The whole module is one rule, stated twice so it cannot be quietly lost:
 *
 * 1. a drop region exists only where the server named a destination; and
 * 2. an action that names none has **no** drop target — it fails closed.
 *
 * The alternative is a client-side table of which card type belongs in which zone,
 * which is game logic (`AGENTS.md`) and is wrong the first time a card says
 * otherwise. Nothing here reads a type line, a mana cost, or a zone's contents.
 *
 * Drag is optional input throughout. These regions describe where a drag *may*
 * release; the same action stays reachable by click, keyboard, and touch, so a
 * surface that renders no region at all is a complete, usable UI.
 */
import type { ActionDestination, EntityId, PlayerId, ValidAction } from '../protocol';

/** The destination kinds this client can render a region for. Anything else is
 * ignored rather than guessed at — a newer server may name kinds we cannot draw. */
const RENDERABLE_KINDS = ['zone', 'entity', 'player'] as const;

/** A destination kind this client knows how to render; one of {@link RENDERABLE_KINDS}. */
export type DropRegionKind = (typeof RENDERABLE_KINDS)[number];

/** One surface a drag may be released on to take {@link DropRegion.actionId}. */
export interface DropRegion {
  /** The `ValidAction.id` this region takes when a drag is released on it. */
  actionId: string;
  /** The action's `token`, echoed verbatim on submission (empty when unbound). */
  token: string;
  /** What surface this names. */
  kind: DropRegionKind;
  /** The zone name, entity id, or player id the region covers. */
  target: EntityId | PlayerId | string;
  /** Whose copy of a per-player zone, when the server named one. */
  owner?: PlayerId;
  /** The server's label for the region, when it supplied one. */
  label?: string;
}

/** Whether a wire destination names a kind this client can render. */
function isRenderable(destination: ActionDestination): destination is ActionDestination & {
  type: DropRegionKind;
} {
  return (RENDERABLE_KINDS as readonly string[]).includes(destination.type);
}

/**
 * The drop regions one action authorizes. Empty — **always** — for an action that
 * names no destination, which is the common case (passing, conceding, a mana ability,
 * every prompt-answered decision) and the correct fail-closed answer for anything a
 * future server adds without a destination.
 */
export function dropRegionsFor(action: ValidAction): DropRegion[] {
  const destinations = action.destinations ?? [];
  const regions: DropRegion[] = [];
  for (const destination of destinations) {
    if (!isRenderable(destination)) continue;
    if (destination.id === '') continue;
    const region: DropRegion = {
      actionId: action.id,
      token: action.token ?? '',
      kind: destination.type,
      target: destination.id,
    };
    if (destination.owner !== undefined && destination.owner !== '')
      region.owner = destination.owner;
    if (destination.label !== undefined && destination.label !== '')
      region.label = destination.label;
    regions.push(region);
  }
  return regions;
}

/**
 * Every drop region the current view authorizes for one dragged entity — the regions
 * a surface highlights while that card is held.
 *
 * The entity is matched against each action's `subject`, the server's own statement
 * of what an action belongs to (ADR 0004); an action with no subject is global and
 * belongs to no dragged card. Dragging something no action names yields an empty
 * list, so nothing lights up and the drop is a no-op.
 */
export function dropRegionsForEntity(
  actions: readonly ValidAction[],
  entityId: EntityId,
): DropRegion[] {
  return actions
    .filter((action) => (action.subject ?? []).includes(entityId))
    .flatMap(dropRegionsFor);
}

/**
 * Whether `region` is the one a release over `kind`/`target` lands on. A plain
 * equality check the caller uses to resolve a drop; it exists so the comparison is
 * written once, and so an owner-scoped zone never matches another seat's copy of it.
 */
export function regionMatches(
  region: DropRegion,
  kind: DropRegionKind,
  target: string,
  owner?: PlayerId,
): boolean {
  if (region.kind !== kind || region.target !== target) return false;
  return region.owner === undefined || region.owner === owner;
}
