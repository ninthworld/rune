/**
 * The directional relationship grammar (`docs/design/stack-and-relationships.md`
 * §4 and §5) as a **pure draw-program builder**.
 *
 * A glowing arc alone says only "these two things are connected". This module
 * adds the four constituents §4.1 requires of every relationship — a source cap,
 * a path with a stated geometry, a direction device, and a destination cap — and
 * renders each of the nine kinds with a geometry of its own, so no two are ever
 * separated by hue alone (invariant I5, §9.4).
 *
 * Direction is carried by three redundant marks, ranked by survivability
 * (§4.2): **D1** endpoint asymmetry (a filled source disc against an open
 * destination cap), **D2** the monotonic stroke taper, and **D3** the dash-crawl
 * flow. D2 is primary because it is the only one that is both *static* (so
 * reduced motion keeps it) and *locally* readable (so a single visible
 * centimetre of stroke still states which way the effect travels behind a
 * crowded board). D3 is exactly what reduced-motion users lose, which is why it
 * ranks last.
 *
 * Nothing here computes game state: every relationship is a pair the server
 * stated, and the kind, order, and destination classification arrive as declared
 * fields on the effect (invariant I1).
 */
import { COMBAT_LINK } from '../../tokens';
import { SCENE_RELATIONSHIP } from '../../sceneTokens';
import { doubledStroke } from '../combatLinks';
import type { Rect } from '../scene';
import {
  arcChords,
  bracketArms,
  chevronWings,
  elbowPath,
  endTangent,
  flowSegments,
  pathCurve,
  rectCenter,
  rectEdgePoint,
  trimEnd,
  trimStart,
  type Point,
} from './geometry';
import type { DrawOp, DrawPart, EndpointKind, PersistentEffect, RelationshipState } from './types';

/** Dash geometry of a pending/provisional path (§4.4). */
export const RELATIONSHIP_DASH = { len: 12, gap: 9 } as const;

/** The §4.3 R4 short hop: a stack → stack path never leaves the stage. */
const STACK_HOP_LIFT = 24;

/**
 * The static kinds. A block is a *bind* rather than a directed effect (D7), and
 * an attachment states "belongs to" rather than "acts on" (D6); neither is ever
 * dashed, crawled, or animated, which is half of why the layer can idle at zero
 * cost while combat is on the board (§8.4).
 */
function isStaticKind(category: PersistentEffect['category']): boolean {
  return (
    category === 'blocker-link' || category === 'attachment-bracket' || category === 'source-tether'
  );
}

/**
 * The declared state of a relationship, or its category default: a targeting
 * path is `pending` (the live-session shape it is only ever built for), and
 * everything else is `confirmed` — combat and attachment are server-stated
 * facts, so they are solid and static.
 */
export function relationshipState(effect: PersistentEffect): RelationshipState {
  if (effect.state !== undefined) return effect.state;
  return effect.category === 'targeting-path' ? 'pending' : 'confirmed';
}

/**
 * Whether this relationship needs a further animation frame — implementation
 * note IN1 and the §8.4 zero-idle contract. Only a **pending** path (which
 * dash-crawls) and a **resolving** path (which retracts) do; confirmed and
 * calmed solid paths, endpoint-only caps, blocker links, and attachment
 * brackets are static geometry and must never mark the layer as animating.
 */
export function relationshipAnimates(effect: PersistentEffect): boolean {
  if (isStaticKind(effect.category)) return false;
  const state = relationshipState(effect);
  return state === 'pending' || state === 'resolving';
}

/** Per-frame inputs the grammar needs but does not own. */
export interface RelationshipContext {
  /** Dash-crawl phase in [0, dash+gap); always 0 under reduced motion. */
  phase: number;
  /** Reduced motion: dashes stand still, the resolving retraction is skipped. */
  reducedMotion: boolean;
  /**
   * The §4.3 R5 fan this path belongs to, when its source has several
   * destinations: `node` is the split point 40 % along the trunk, and `trunk`
   * marks the one member that draws the shared trunk, source disc, and node.
   */
  fan?: { node: Point; trunk: boolean };
  /** Retraction progress in [0, 1] while the state is `resolving` (§6.2). */
  progress?: number;
}

/** The alpha a state renders at (§4.4). */
function stateAlpha(state: RelationshipState): number {
  const alpha = SCENE_RELATIONSHIP.alpha;
  switch (state) {
    case 'calmed':
      return alpha.calmed;
    case 'endpoint-only':
      return alpha.endpointOnly;
    case 'provisional':
      return alpha.provisional;
    case 'confirmed':
      return alpha.confirmed;
    case 'resolving':
      return alpha.resolving;
    default:
      return alpha.pending;
  }
}

/** The cap radius a destination kind wears, fitted to the destination's rect. */
function capRadius(kind: EndpointKind, rect: Rect): number {
  const half = Math.min(rect.w, rect.h) / 2;
  switch (kind) {
    // The crest IS the ring (§5.3): the arc rides the crest's own radius.
    case 'player':
      return Math.max(8, half);
    case 'stack':
      return Math.min(SCENE_RELATIONSHIP.reticleInsetRadius, Math.max(4, half));
    case 'zone':
      return Math.max(6, Math.min(SCENE_RELATIONSHIP.bracketSpine / 2, half));
    default:
      return Math.min(SCENE_RELATIONSHIP.reticleRadius, Math.max(6, half * 0.9));
  }
}

function segment(
  category: string,
  part: DrawPart,
  from: Point,
  to: Point,
  color: string,
  width: number,
  alpha: number,
): DrawOp {
  return { op: 'segment', category, part, from, to, color, width, alpha };
}

function circle(
  category: string,
  part: DrawPart,
  at: Point,
  r: number,
  color: string,
  alpha: number,
  fill: boolean,
): DrawOp {
  return { op: 'circle', category, part, x: at.x, y: at.y, r, color, alpha, fill };
}

/** The taper's width at normalized arc position `t` (§4.2 D2). */
function taperWidth(t: number): number {
  const { taperFrom, taperTo } = SCENE_RELATIONSHIP;
  return taperFrom + (taperTo - taperFrom) * Math.min(1, Math.max(0, t));
}

/**
 * The §10.3 edge indicator: a chevron on the container edge an occluded endpoint
 * was clamped to, pointing along the path's would-be tangent. The relationship
 * is never silently lost — it terminates visibly at the edge instead.
 */
export function edgeIndicatorOps(
  category: string,
  at: Point,
  angle: number,
  color: string,
  alpha: number,
): DrawOp[] {
  return chevronWings(at, angle, SCENE_RELATIONSHIP.edgeIndicator).map(([a, b]) =>
    segment(category, 'edge', a, b, color, SCENE_RELATIONSHIP.crestWidth, alpha),
  );
}

/** The §4.5 numeral pips: the destination's 1-based place in the server's own
 * target list, as a countable shape channel beside the cap. */
function numeralOps(effect: PersistentEffect, tip: Point, angle: number, alpha: number): DrawOp[] {
  const numeral = effect.numeral ?? 0;
  if (numeral < 1) return [];
  const { numeralPip, numeralPitch } = SCENE_RELATIONSHIP;
  const px = -Math.sin(angle);
  const py = Math.cos(angle);
  const ops: DrawOp[] = [];
  for (let i = 0; i < numeral; i += 1) {
    const offset = (i - (numeral - 1) / 2) * numeralPitch;
    ops.push(
      circle(
        effect.category,
        'numeral',
        { x: tip.x + px * offset, y: tip.y + py * offset },
        numeralPip,
        effect.accent,
        alpha,
        true,
      ),
    );
  }
  return ops;
}

/** The destination cap of a path, by endpoint kind (§5.2–§5.5). */
function destinationOps(
  effect: PersistentEffect,
  kind: EndpointKind,
  center: Point,
  tip: Point,
  angle: number,
  radius: number,
  alpha: number,
): DrawOp[] {
  const { category, accent } = effect;
  const ops: DrawOp[] = [];
  if (kind === 'player') {
    // §5.3 / D8 — a crest is already a circle, so a concentric reticle would
    // read as decoration. The arc reads as a hit on the shield.
    const arcAngle = Math.atan2(tip.y - center.y, tip.x - center.x);
    for (const [a, b] of arcChords(
      center,
      radius,
      arcAngle,
      SCENE_RELATIONSHIP.crestSweep,
      SCENE_RELATIONSHIP.crestChords,
    )) {
      ops.push(segment(category, 'cap', a, b, accent, SCENE_RELATIONSHIP.crestWidth, alpha));
    }
    for (const [a, b] of chevronWings(tip, angle, SCENE_RELATIONSHIP.chevron)) {
      ops.push(segment(category, 'cap', a, b, accent, SCENE_RELATIONSHIP.crestWidth, alpha));
    }
    return ops;
  }
  if (kind === 'zone') {
    // §5.4 / D9 — a zone is a container, not a body: a rectilinear bracket.
    for (const [a, b] of bracketArms(
      tip,
      angle,
      SCENE_RELATIONSHIP.bracketArm,
      SCENE_RELATIONSHIP.bracketSpine,
    )) {
      ops.push(segment(category, 'cap', a, b, accent, SCENE_RELATIONSHIP.reticleWidth, alpha));
    }
    for (const [a, b] of chevronWings(tip, angle, SCENE_RELATIONSHIP.chevron * 0.75)) {
      ops.push(segment(category, 'cap', a, b, accent, SCENE_RELATIONSHIP.reticleWidth, alpha));
    }
    return ops;
  }
  // §5.2 / §5.5 — the open reticle, with the arrowhead landing INSIDE the ring
  // as one inward chevron on the arrival tangent. `stack` uses the inset radius
  // so the cap stays within its slot's bounds.
  ops.push(circle(category, 'cap', center, radius, accent, alpha, false));
  const chevron = Math.min(SCENE_RELATIONSHIP.chevron, radius * 0.9);
  for (const [a, b] of chevronWings(center, angle, chevron)) {
    ops.push(segment(category, 'cap', a, b, accent, SCENE_RELATIONSHIP.reticleWidth, alpha));
  }
  return ops;
}

/**
 * The R9 elbow bracket (`attachment-bracket` / `source-tether`): two
 * axis-aligned right-angle connectors with **symmetric square terminals**, drawn
 * in line-work neutral. Symmetric caps and rectilinear geometry are the whole
 * point — this shape must never be mistaken for a directed target path (D6).
 */
function elbowOps(effect: PersistentEffect, from: Rect, to: Rect): DrawOp[] {
  const a = rectEdgePoint(from, rectCenter(to));
  const b = rectEdgePoint(to, rectCenter(from));
  const alpha = SCENE_RELATIONSHIP.lineworkAlpha;
  const side = SCENE_RELATIONSHIP.terminal;
  const ops: DrawOp[] = elbowPath(a, b).map(([p, q]) =>
    segment(effect.category, 'path', p, q, effect.accent, SCENE_RELATIONSHIP.elbowWidth, alpha),
  );
  for (const at of [a, b]) {
    ops.push({
      op: 'rect',
      category: effect.category,
      part: 'terminal',
      x: at.x - side / 2,
      y: at.y - side / 2,
      w: side,
      h: side,
      color: effect.accent,
      alpha,
      fill: true,
    });
  }
  return ops;
}

/** The carried R8 blocker link: a straight doubled stroke with a node at the
 * blocker end and **no arrowhead** — the absence is the semantic (D7). */
function blockerOps(effect: PersistentEffect, from: Point, to: Point, alpha: number): DrawOp[] {
  const ops: DrawOp[] = doubledStroke(from, to).map(([a, b]) =>
    segment(effect.category, 'path', a, b, effect.accent, COMBAT_LINK.strokeWidth, alpha),
  );
  ops.push(
    circle(effect.category, 'source', from, COMBAT_LINK.nodeRadius, effect.accent, alpha, true),
  );
  return ops;
}

/**
 * Build the complete draw program of one relationship, from its two resolved
 * rects. Pure and deterministic: the same inputs always produce the same ops,
 * which is what makes the whole grammar assertable GPU-free (IN7).
 */
export function relationshipOps(
  effect: PersistentEffect,
  from: Rect,
  to: Rect,
  ctx: RelationshipContext,
): DrawOp[] {
  if (effect.category === 'attachment-bracket' || effect.category === 'source-tether') {
    return elbowOps(effect, from, to);
  }

  const state = relationshipState(effect);
  const alpha = stateAlpha(state);
  const fromCenter = rectCenter(from);
  const toCenter = rectCenter(to);

  if (effect.category === 'blocker-link') {
    return blockerOps(effect, fromCenter, toCenter, alpha);
  }

  const kind = effect.endpoint ?? 'card';
  const radius = capRadius(kind, to);
  const source = rectEdgePoint(from, toCenter);
  const fan = ctx.fan;
  const drawsSourceCap = fan === undefined || fan.trunk;
  const ops: DrawOp[] = [];

  // D1 — the filled source disc. A fan group shares one disc (§4.3 R5).
  if (drawsSourceCap) {
    ops.push(
      circle(
        effect.category,
        'source',
        source,
        SCENE_RELATIONSHIP.sourceRadius,
        effect.accent,
        alpha,
        true,
      ),
    );
  }

  // §4.4 endpoint-only — the crowded-board floor (D11): both caps, no stroke.
  // Exactly two circle ops, whatever the destination kind.
  if (state === 'endpoint-only') {
    ops.push(circle(effect.category, 'cap', toCenter, radius, effect.accent, alpha, false));
    return ops;
  }

  const lift = kind === 'stack' ? STACK_HOP_LIFT : undefined;
  const origin = fan ? fan.node : source;
  const curve = trimEnd(pathCurve(origin, toCenter, 24, lift), radius);

  // The trunk of a multi-target fan: one stroke from the shared source to the
  // split, plus the hollow node that states "one source, many subjects" (D12).
  if (fan !== undefined && fan.trunk) {
    const trunk = pathCurve(source, fan.node, 8, 0);
    const trunkSegments = flowSegments(trunk, 0, 0, 0);
    for (const piece of trunkSegments) {
      ops.push(
        segment(
          effect.category,
          'trunk',
          piece.from,
          piece.to,
          effect.accent,
          taperWidth(piece.t * SCENE_RELATIONSHIP.fanAt),
          alpha,
        ),
      );
    }
    ops.push(
      circle(
        effect.category,
        'fan',
        fan.node,
        SCENE_RELATIONSHIP.fanRadius,
        effect.accent,
        alpha,
        false,
      ),
    );
  }

  // §6.2 — the resolving retraction pulls the stroke off the source and onto the
  // destination cap. Reduced motion drops the travel entirely (F6): the path is
  // simply gone in the same frame the state applies.
  const progress = state === 'resolving' ? Math.min(1, Math.max(0, ctx.progress ?? 0)) : 0;
  if (state === 'resolving' && ctx.reducedMotion) return ops;
  const drawn = progress > 0 ? trimStart(curve, progress) : curve;
  const strokeAlpha = state === 'resolving' ? alpha * (1 - progress) : alpha;

  // D3 — flow. Pending crawls; provisional holds phase 0; everything else is
  // solid. Reduced motion keeps the dash PATTERN (pending vs confirmed stays
  // legible) and only stops the crawl (§7.2).
  const dash = state === 'pending' || state === 'provisional' ? RELATIONSHIP_DASH.len : 0;
  const phase = state === 'pending' && !ctx.reducedMotion ? ctx.phase : 0;
  const base = fan ? SCENE_RELATIONSHIP.fanAt : 0;
  const span = 1 - base;
  for (const piece of flowSegments(drawn, dash, RELATIONSHIP_DASH.gap, phase)) {
    // A fully retracted path leaves a degenerate stub; it draws nothing, so it
    // must not cost an op either.
    if (piece.from.x === piece.to.x && piece.from.y === piece.to.y) continue;
    // D2 — the monotonic taper, mapped through the fan's and the retraction's
    // remaining span so a partial stroke keeps the same width profile.
    const t = base + span * (progress + (1 - progress) * piece.t);
    ops.push(
      segment(
        effect.category,
        'path',
        piece.from,
        piece.to,
        effect.accent,
        taperWidth(t),
        strokeAlpha,
      ),
    );
  }

  const tip = drawn[drawn.length - 1]!;
  const angle = endTangent(drawn);
  ops.push(...destinationOps(effect, kind, toCenter, tip, angle, radius, strokeAlpha));
  ops.push(...numeralOps(effect, tip, angle, strokeAlpha));
  return ops;
}

/** One resolved relationship the fan pass reads. */
export interface ResolvedRelationship {
  effect: PersistentEffect;
  /** The source cap's point (the source rect's edge toward the destination). */
  source: Point;
  /** The destination's center. */
  destination: Point;
}

/**
 * The §4.3 R5 / D12 multi-target fan: when one source has several destinations,
 * the paths leave as **one trunk** and split at a fan node 40 % along it, one
 * branch per target. N independent arcs from one source form a starburst that
 * hides the source; the node states "one source, many subjects" instead.
 *
 * Pure over already-resolved geometry, so it is unit-testable without a GPU.
 */
export function fanGroups(
  resolved: readonly ResolvedRelationship[],
): Map<string, { node: Point; trunk: boolean }> {
  const groups = new Map<string, ResolvedRelationship[]>();
  for (const entry of resolved) {
    if (entry.effect.category !== 'targeting-path') continue;
    const key = `${entry.source.x},${entry.source.y}`;
    const group = groups.get(key);
    if (group) group.push(entry);
    else groups.set(key, [entry]);
  }
  const out = new Map<string, { node: Point; trunk: boolean }>();
  for (const group of groups.values()) {
    if (group.length < 2) continue;
    const source = group[0]!.source;
    const centroid = {
      x: group.reduce((sum, e) => sum + e.destination.x, 0) / group.length,
      y: group.reduce((sum, e) => sum + e.destination.y, 0) / group.length,
    };
    const at = SCENE_RELATIONSHIP.fanAt;
    const node = {
      x: source.x + (centroid.x - source.x) * at,
      y: source.y + (centroid.y - source.y) * at,
    };
    // The trunk owner is stable across frames: the lowest id, never screen order.
    const owner = group.reduce((a, b) => (a.effect.id <= b.effect.id ? a : b)).effect.id;
    for (const entry of group) {
      out.set(entry.effect.id, { node, trunk: entry.effect.id === owner });
    }
  }
  return out;
}
