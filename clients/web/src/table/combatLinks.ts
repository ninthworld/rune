/**
 * Pure policy and geometry for the blocker→attacker combat links (issue #339).
 *
 * The scene already computes the blocker→attacker relationships from the view alone
 * ({@link TableScene.combatLinks}); this module decides *which* of them to draw and
 * *where*, so the drawing layer (the Pixi reconciler) stays a thin renderer and the
 * density/isolation policy is unit-testable without a GPU. No combat is computed here
 * — a link is exactly the server's `blocking` reference.
 */
import type { EntityId } from '../protocol';
import type { CombatLink } from './scene';
import { COMBAT_LINK } from '../tokens';

/** A link one of whose endpoints could be located, with both endpoints' centers. */
export interface PositionedLink {
  link: CombatLink;
  /** The blocking permanent's center (scene coords). */
  from: { x: number; y: number };
  /** The attacked permanent's center (scene coords). */
  to: { x: number; y: number };
}

/** Whether a link touches `id` as either its blocker or its attacker. */
export function linkTouches(link: CombatLink, id: EntityId): boolean {
  return link.blocker === id || link.attacker === id;
}

/**
 * Which links to draw at full emphasis given the current isolation.
 *
 * - With a participant **isolated** (focused / selected / hovered), only that
 *   object's links are drawn — the requirement that focus isolates one object's links
 *   instead of drawing every line at once on a crowded board.
 * - With nothing isolated, every link is drawn (calmed to {@link COMBAT_LINK.crowdedAlpha}
 *   when the board is crowded — see {@link linkAlpha}).
 */
export function selectVisibleLinks(links: CombatLink[], isolatedId: EntityId | null): CombatLink[] {
  if (isolatedId == null) return links;
  return links.filter((link) => linkTouches(link, isolatedId));
}

/**
 * The alpha links render at: full when a participant is isolated or the board is not
 * crowded; calmed when many links would otherwise all draw at once with nothing
 * focused, so the board stays legible until focus isolates one object's links.
 */
export function linkAlpha(totalLinks: number, isolatedId: EntityId | null): number {
  if (isolatedId != null) return COMBAT_LINK.alpha;
  return totalLinks > COMBAT_LINK.crowdedThreshold ? COMBAT_LINK.crowdedAlpha : COMBAT_LINK.alpha;
}

/**
 * Resolve each link's endpoints from a `centerOf` lookup (the reconciler passes the
 * card's *current* on-screen center, so links track their endpoints while a view-diff
 * animation is in flight — issue #334). A link whose blocker or attacker cannot be
 * located (it has left play, or is off this client's board) is dropped rather than
 * drawn to a stale point.
 */
export function positionLinks(
  links: CombatLink[],
  centerOf: (id: EntityId) => { x: number; y: number } | undefined,
): PositionedLink[] {
  const out: PositionedLink[] = [];
  for (const link of links) {
    const from = centerOf(link.blocker);
    const to = centerOf(link.attacker);
    if (from && to) out.push({ link, from, to });
  }
  return out;
}

/**
 * The two parallel stroke offsets for the doubled combat-link line between `from` and
 * `to`. Each returned pair is a `[start, end]` the renderer strokes; together the two
 * parallel lines read as a combat "bind", a different *shape* than a single targeting
 * arrow. Perpendicular offset is half the configured gap on each side of the centre
 * line. A degenerate zero-length link (both endpoints coincident) returns the centre
 * line twice, which draws nothing visible — safe.
 */
export function doubledStroke(
  from: { x: number; y: number },
  to: { x: number; y: number },
): Array<[{ x: number; y: number }, { x: number; y: number }]> {
  const dx = to.x - from.x;
  const dy = to.y - from.y;
  const len = Math.hypot(dx, dy);
  if (len === 0) {
    return [
      [from, to],
      [from, to],
    ];
  }
  // Unit perpendicular to the link direction.
  const px = -dy / len;
  const py = dx / len;
  const half = COMBAT_LINK.gap / 2;
  return [
    [
      { x: from.x + px * half, y: from.y + py * half },
      { x: to.x + px * half, y: to.y + py * half },
    ],
    [
      { x: from.x - px * half, y: from.y - py * half },
      { x: to.x - px * half, y: to.y - py * half },
    ],
  ];
}
