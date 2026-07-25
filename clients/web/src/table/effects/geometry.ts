import type { Rect } from '../scene';
import type { EffectAnchor } from './types';

/** A 2D point. */
export interface Point {
  x: number;
  y: number;
}

/** The center of a rect. */
export function rectCenter(rect: Rect): Point {
  return { x: rect.x + rect.w / 2, y: rect.y + rect.h / 2 };
}

/**
 * Resolve an anchor to its current center: a fixed rect directly, a live ref
 * through the rect source (the endpoint-tracking seam — the source reports
 * *current* rects, so paths follow their endpoints while reconciler motion is
 * in flight, exactly like the shipped combat-link overlay). An unresolvable
 * ref returns `undefined` and its effect is dropped rather than drawn stale.
 */
export function anchorCenter(
  anchor: EffectAnchor,
  rects: (ref: string) => Rect | undefined,
): Point | undefined {
  if ('rect' in anchor) return rectCenter(anchor.rect);
  const rect = rects(anchor.ref);
  return rect ? rectCenter(rect) : undefined;
}

/**
 * Resolve an anchor to its current **rect**. Endpoint treatments need more than
 * a point: the source cap sits on the source's bounding-rect edge (§5.1), the
 * crest arc's radius comes from the crest's own rect (§5.3), and the zone
 * bracket is laid over the pile (§5.4). An unresolvable ref returns `undefined`
 * exactly as {@link anchorCenter} does.
 */
export function anchorRect(
  anchor: EffectAnchor,
  rects: (ref: string) => Rect | undefined,
): Rect | undefined {
  return 'rect' in anchor ? anchor.rect : rects(anchor.ref);
}

/** The default corridor lift of a relationship path (§4.3 R1, `pathCurve`). */
export const PATH_LIFT = 80;

/**
 * The targeting/attack path curve: a quadratic bezier lifted above its
 * endpoints (the carried arc shape — the path rises over the corridor and
 * terminates at the target's crest/object), sampled as a polyline. `lift`
 * carries the §4.3 R4 short hop (24 px, a counterspell's arc between two stack
 * entries) and, when a caller measures it, the §10.1 six-seat corridor raise.
 */
export function pathCurve(from: Point, to: Point, samples = 24, lift = PATH_LIFT): Point[] {
  const control = {
    x: (from.x + to.x) / 2,
    y: Math.min(from.y, to.y) - lift,
  };
  const points: Point[] = [];
  for (let i = 0; i <= samples; i += 1) {
    const t = i / samples;
    const u = 1 - t;
    points.push({
      x: u * u * from.x + 2 * u * t * control.x + t * t * to.x,
      y: u * u * from.y + 2 * u * t * control.y + t * t * to.y,
    });
  }
  return points;
}

/** The total arc length of a sampled polyline. */
export function polylineLength(points: Point[]): number {
  let total = 0;
  for (let i = 1; i < points.length; i += 1) {
    total += Math.hypot(points[i]!.x - points[i - 1]!.x, points[i]!.y - points[i - 1]!.y);
  }
  return total;
}

/**
 * One drawn piece of a path, carrying **where along the path it sits**.
 *
 * `t` is the normalized arc position of the piece's midpoint — 0 at the source,
 * 1 at the destination. It is what makes the monotonic stroke taper
 * (`stack-and-relationships.md` §4.2 device D2) computable for a dashed path:
 * every visible dash states which way the relationship travels on its own,
 * without needing either endpoint to be on screen.
 */
export interface FlowSegment {
  from: Point;
  to: Point;
  /** Normalized arc position of the midpoint, source 0 → destination 1. */
  t: number;
}

/**
 * Cut a polyline into drawn pieces with their arc positions.
 *
 * `dash <= 0` renders the **solid** form (one piece per polyline sample, the
 * confirmed/calmed/resolving states); a positive `dash` cuts the dash-crawl of
 * a pending path, where `phase` in [0, dash+gap) shifts every dash along the
 * line and a fixed phase renders the static reduced-motion form.
 */
export function flowSegments(
  points: Point[],
  dash: number,
  gap: number,
  phase: number,
): FlowSegment[] {
  const total = polylineLength(points);
  const at = (distance: number): number => (total === 0 ? 0 : distance / total);
  const segments: FlowSegment[] = [];
  if (dash <= 0) {
    let travelledTotal = 0;
    for (let i = 1; i < points.length; i += 1) {
      const a = points[i - 1]!;
      const b = points[i]!;
      const len = Math.hypot(b.x - a.x, b.y - a.y);
      segments.push({ from: a, to: b, t: at(travelledTotal + len / 2) });
      travelledTotal += len;
    }
    return segments;
  }
  const period = dash + gap;
  const inPeriod = (d: number): number => ((d % period) + period) % period;
  let distance = -inPeriod(phase);
  let travelledTotal = 0;
  let open: Point | null = inPeriod(distance) < dash ? points[0]! : null;
  let openAt = 0;
  for (let i = 1; i < points.length; i += 1) {
    const a = points[i - 1]!;
    const b = points[i]!;
    const len = Math.hypot(b.x - a.x, b.y - a.y);
    let travelled = 0;
    while (travelled < len) {
      const phaseAt = inPeriod(distance);
      // Distance until the dash/gap boundary ahead.
      const boundary = phaseAt < dash ? dash - phaseAt : period - phaseAt;
      const step = Math.min(boundary, len - travelled);
      const t0 = travelled / len;
      const t1 = (travelled + step) / len;
      const p0 = { x: a.x + (b.x - a.x) * t0, y: a.y + (b.y - a.y) * t0 };
      const p1 = { x: a.x + (b.x - a.x) * t1, y: a.y + (b.y - a.y) * t1 };
      if (phaseAt < dash) {
        if (open === null) {
          open = p0;
          openAt = travelledTotal + travelled;
        }
        if (step === boundary) {
          const closeAt = travelledTotal + travelled + step;
          segments.push({ from: open, to: p1, t: at((openAt + closeAt) / 2) });
          open = null;
        }
      }
      travelled += step;
      distance += step;
    }
    travelledTotal += len;
  }
  if (open !== null) {
    segments.push({ from: open, to: points[points.length - 1]!, t: at((openAt + total) / 2) });
  }
  return segments;
}

/**
 * Cut a polyline into dash segments with a moving phase offset — the
 * dash-crawl of a pending targeting session. `phase` in [0, dash+gap) shifts
 * every dash along the line; a fixed phase renders the static reduced-motion
 * form. The arc-position-carrying {@link flowSegments} is the same cut.
 */
export function dashSegments(
  points: Point[],
  dash: number,
  gap: number,
  phase: number,
): Array<[Point, Point]> {
  return flowSegments(points, dash, gap, phase).map((segment) => [segment.from, segment.to]);
}

/**
 * The point on a rect's boundary in the direction of `toward` — where a source
 * cap sits (`stack-and-relationships.md` §5.1: "on the source object's
 * bounding-rect edge, at the point nearest the destination"). A `toward` at the
 * rect's own center degenerates to that center.
 */
export function rectEdgePoint(rect: Rect, toward: Point): Point {
  const center = rectCenter(rect);
  const dx = toward.x - center.x;
  const dy = toward.y - center.y;
  if (dx === 0 && dy === 0) return center;
  const scale = Math.min(
    dx === 0 ? Infinity : rect.w / 2 / Math.abs(dx),
    dy === 0 ? Infinity : rect.h / 2 / Math.abs(dy),
  );
  return { x: center.x + dx * scale, y: center.y + dy * scale };
}

/**
 * The nearest point on a rect's **boundary** to `point` — the §10.3 clamp for
 * an endpoint that is occluded rather than gone: the path terminates on the
 * container's edge with its normal cap, so direction stays readable.
 */
export function clampToRect(rect: Rect, point: Point): Point {
  const x = Math.min(Math.max(point.x, rect.x), rect.x + rect.w);
  const y = Math.min(Math.max(point.y, rect.y), rect.y + rect.h);
  const inside =
    point.x > rect.x && point.x < rect.x + rect.w && point.y > rect.y && point.y < rect.y + rect.h;
  if (!inside) return { x, y };
  // Inside: push out along the shortest axis so the terminus lands on an edge.
  const left = point.x - rect.x;
  const right = rect.x + rect.w - point.x;
  const top = point.y - rect.y;
  const bottom = rect.y + rect.h - point.y;
  const min = Math.min(left, right, top, bottom);
  if (min === left) return { x: rect.x, y: point.y };
  if (min === right) return { x: rect.x + rect.w, y: point.y };
  if (min === top) return { x: point.x, y: rect.y };
  return { x: point.x, y: rect.y + rect.h };
}

/**
 * Trim the tail of a sampled path so it stops exactly `radius` short of its own
 * last point — how a path terminates *at* its destination cap (the reticle ring,
 * the crest arc, the zone bracket's spine) rather than running under it. The
 * trimmed path's end tangent is the cap's arrival tangent.
 */
export function trimEnd(points: Point[], radius: number): Point[] {
  const tip = points[points.length - 1]!;
  if (radius <= 0 || points.length < 2) return points;
  for (let i = points.length - 2; i >= 0; i -= 1) {
    const p = points[i]!;
    const distance = Math.hypot(tip.x - p.x, tip.y - p.y);
    if (distance < radius) continue;
    const next = points[i + 1]!;
    const span = Math.hypot(next.x - p.x, next.y - p.y);
    const remaining = distance - radius;
    const f = span === 0 ? 0 : Math.min(1, remaining / span);
    return [
      ...points.slice(0, i + 1),
      { x: p.x + (next.x - p.x) * f, y: p.y + (next.y - p.y) * f },
    ];
  }
  // The whole path is inside the cap: keep a degenerate two-point stub so the
  // caller still has a tangent and never divides by zero.
  return [points[0]!, points[0]!];
}

/** Trim the head of a path — the `resolving` retraction (§6.2), which pulls the
 * stroke away from the source and toward the destination cap. */
export function trimStart(points: Point[], fraction: number): Point[] {
  if (fraction <= 0) return points;
  if (fraction >= 1) return [points[points.length - 1]!, points[points.length - 1]!];
  const total = polylineLength(points);
  let target = total * fraction;
  for (let i = 1; i < points.length; i += 1) {
    const a = points[i - 1]!;
    const b = points[i]!;
    const len = Math.hypot(b.x - a.x, b.y - a.y);
    if (len < target) {
      target -= len;
      continue;
    }
    const f = len === 0 ? 0 : target / len;
    return [{ x: a.x + (b.x - a.x) * f, y: a.y + (b.y - a.y) * f }, ...points.slice(i)];
  }
  return [points[points.length - 1]!, points[points.length - 1]!];
}

/** The angle of a path's final segment — every destination cap's orientation. */
export function endTangent(points: Point[]): number {
  const tip = points[points.length - 1]!;
  const prev = points[Math.max(0, points.length - 2)]!;
  if (tip.x === prev.x && tip.y === prev.y) return 0;
  return Math.atan2(tip.y - prev.y, tip.x - prev.x);
}

/**
 * A chevron's two wings, from `tip` and trailing back along `angle`. This is the
 * arrowhead form used both outside a crest arc and **inside** a target reticle
 * (§5.2: "the arrowhead lands inside the ring").
 */
export function chevronWings(tip: Point, angle: number, size: number): Array<[Point, Point]> {
  const wing = (offset: number): Point => ({
    x: tip.x - size * Math.cos(angle + offset),
    y: tip.y - size * Math.sin(angle + offset),
  });
  return [
    [tip, wing(0.4)],
    [tip, wing(-0.4)],
  ];
}

/**
 * A circular arc as chords — the §5.3 player endpoint, a 90° segment on the
 * seat crest's ring centred on the path's arrival tangent. There is no arc
 * primitive in the draw program; chords keep the cap inside the single pooled
 * `Graphics` (§8.1, "one draw call").
 */
export function arcChords(
  center: Point,
  radius: number,
  angle: number,
  sweep: number,
  chords: number,
): Array<[Point, Point]> {
  const at = (a: number): Point => ({
    x: center.x + Math.cos(a) * radius,
    y: center.y + Math.sin(a) * radius,
  });
  const start = angle - sweep / 2;
  const out: Array<[Point, Point]> = [];
  for (let i = 0; i < chords; i += 1) {
    out.push([at(start + (sweep * i) / chords), at(start + (sweep * (i + 1)) / chords)]);
  }
  return out;
}

/**
 * The §5.4 zone endpoint: a **square bracket** — a spine across the arriving
 * path with one arm at each end, opening toward the path. A zone is a container,
 * not a body, so its cap is rectilinear (D9) and can never be read as a reticle.
 */
export function bracketArms(
  center: Point,
  angle: number,
  arm: number,
  spine: number,
): Array<[Point, Point]> {
  // The spine is perpendicular to the arrival direction; the arms run back
  // along it, so the bracket opens toward the incoming path.
  const px = -Math.sin(angle);
  const py = Math.cos(angle);
  const half = spine / 2;
  const a = { x: center.x + px * half, y: center.y + py * half };
  const b = { x: center.x - px * half, y: center.y - py * half };
  const back = (p: Point): Point => ({
    x: p.x - Math.cos(angle) * arm,
    y: p.y - Math.sin(angle) * arm,
  });
  return [
    [a, b],
    [a, back(a)],
    [b, back(b)],
  ];
}

/**
 * The §4.3 R9 **elbow bracket pair**: two axis-aligned right-angle connectors,
 * one per direction. This is the shape that says "attached / belongs to", and it
 * is never an arc (decision D6) — that separation from a target path is hard.
 */
export function elbowPath(from: Point, to: Point): Array<[Point, Point]> {
  const corner = { x: to.x, y: from.y };
  const mirror = { x: from.x, y: to.y };
  return [
    [from, corner],
    [corner, to],
    [from, mirror],
    [mirror, to],
  ];
}

/** The two arrowhead strokes at the path's terminus, along its end tangent. */
export function arrowHead(points: Point[], size = 12): Array<[Point, Point]> {
  const tip = points[points.length - 1]!;
  const prev = points[Math.max(0, points.length - 2)]!;
  const angle = Math.atan2(tip.y - prev.y, tip.x - prev.x);
  const wing = (offset: number): Point => ({
    x: tip.x - size * Math.cos(angle + offset),
    y: tip.y - size * Math.sin(angle + offset),
  });
  return [
    [tip, wing(0.4)],
    [tip, wing(-0.4)],
  ];
}

/** One deterministic burst particle: its direction, speed and size. */
export interface BurstParticle {
  angle: number;
  speed: number;
  size: number;
}

/** The golden angle, radians — spreads particles evenly with no randomness. */
const GOLDEN_ANGLE = 2.399963229728653;

/**
 * The particle seeds of an impact burst — **deterministic** (golden-angle
 * spread, index-derived speed/size): the same invocation always bursts the
 * same way, so structural snapshots and replays never flake.
 */
export function burstParticles(count: number): BurstParticle[] {
  const particles: BurstParticle[] = [];
  for (let i = 0; i < count; i += 1) {
    particles.push({
      angle: i * GOLDEN_ANGLE,
      speed: 40 + (i % 5) * 14,
      size: 2 + (i % 3),
    });
  }
  return particles;
}
