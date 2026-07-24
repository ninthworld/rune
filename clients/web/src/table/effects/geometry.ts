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
 * The targeting/attack path curve: a quadratic bezier lifted above its
 * endpoints (the carried arc shape — the path rises over the corridor and
 * terminates at the target's crest/object), sampled as a polyline.
 */
export function pathCurve(from: Point, to: Point, samples = 24): Point[] {
  const control = {
    x: (from.x + to.x) / 2,
    y: Math.min(from.y, to.y) - 80,
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

/**
 * Cut a polyline into dash segments with a moving phase offset — the
 * dash-crawl of a pending targeting session. `phase` in [0, dash+gap) shifts
 * every dash along the line; a fixed phase renders the static reduced-motion
 * form.
 */
export function dashSegments(
  points: Point[],
  dash: number,
  gap: number,
  phase: number,
): Array<[Point, Point]> {
  const period = dash + gap;
  const segments: Array<[Point, Point]> = [];
  const inPeriod = (d: number): number => ((d % period) + period) % period;
  let distance = -inPeriod(phase);
  let open: Point | null = inPeriod(distance) < dash ? points[0]! : null;
  for (let i = 1; i < points.length; i += 1) {
    const a = points[i - 1]!;
    const b = points[i]!;
    const len = Math.hypot(b.x - a.x, b.y - a.y);
    let travelled = 0;
    while (travelled < len) {
      const at = inPeriod(distance);
      // Distance until the dash/gap boundary ahead.
      const boundary = at < dash ? dash - at : period - at;
      const step = Math.min(boundary, len - travelled);
      const t0 = travelled / len;
      const t1 = (travelled + step) / len;
      const p0 = { x: a.x + (b.x - a.x) * t0, y: a.y + (b.y - a.y) * t0 };
      const p1 = { x: a.x + (b.x - a.x) * t1, y: a.y + (b.y - a.y) * t1 };
      if (at < dash) {
        if (open === null) open = p0;
        if (step === boundary) {
          segments.push([open, p1]);
          open = null;
        }
      }
      travelled += step;
      distance += step;
    }
  }
  if (open !== null) segments.push([open, points[points.length - 1]!]);
  return segments;
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
