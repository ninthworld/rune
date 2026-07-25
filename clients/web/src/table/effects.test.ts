/**
 * Effects geometry and vocabulary policy (issue #482): pure, deterministic
 * pieces of the passive effects layer — anchors, the path curve, dash-crawl
 * segmentation, arrowheads, and the seeded (never random) burst particles.
 */
import { describe, expect, it } from 'vitest';
import {
  anchorCenter,
  arcChords,
  arrowHead,
  bracketArms,
  burstParticles,
  clampToRect,
  dashSegments,
  elbowPath,
  endTangent,
  flowSegments,
  pathCurve,
  polylineLength,
  rectCenter,
  rectEdgePoint,
  trimEnd,
  trimStart,
} from './effects';

describe('effect anchors', () => {
  it('resolves a fixed rect to its center', () => {
    expect(rectCenter({ x: 10, y: 20, w: 40, h: 60 })).toEqual({ x: 30, y: 50 });
    expect(anchorCenter({ rect: { x: 0, y: 0, w: 10, h: 10 } }, () => undefined)).toEqual({
      x: 5,
      y: 5,
    });
  });

  it('resolves a live ref through the rect source, dropping the unresolvable', () => {
    const rects = (ref: string) => (ref === 'e1' ? { x: 100, y: 100, w: 20, h: 20 } : undefined);
    expect(anchorCenter({ ref: 'e1' }, rects)).toEqual({ x: 110, y: 110 });
    // A ref that left play resolves to nothing — never a stale endpoint.
    expect(anchorCenter({ ref: 'gone' }, rects)).toBeUndefined();
  });
});

describe('path curve (targeting/attack)', () => {
  it('starts and ends exactly on its endpoints and lifts over the corridor', () => {
    const from = { x: 100, y: 400 };
    const to = { x: 500, y: 300 };
    const points = pathCurve(from, to);
    expect(points[0]).toEqual(from);
    expect(points[points.length - 1]).toEqual(to);
    // The arc rises above both endpoints (the lifted corridor shape).
    const minY = Math.min(...points.map((p) => p.y));
    expect(minY).toBeLessThan(Math.min(from.y, to.y));
  });
});

describe('dash-crawl segmentation', () => {
  const line = [
    { x: 0, y: 0 },
    { x: 100, y: 0 },
  ];

  it('cuts a line into alternating dashes at phase 0', () => {
    const segments = dashSegments(line, 10, 10, 0);
    expect(segments[0]![0]).toEqual({ x: 0, y: 0 });
    expect(segments[0]![1]).toEqual({ x: 10, y: 0 });
    expect(segments[1]![0]).toEqual({ x: 20, y: 0 });
    expect(segments).toHaveLength(5);
  });

  it('shifts the pattern with phase — the crawl frame-over-frame', () => {
    const a = dashSegments(line, 10, 10, 0);
    const b = dashSegments(line, 10, 10, 5);
    expect(a).not.toEqual(b);
    // Same phase modulo the period renders identically (a stable loop).
    expect(dashSegments(line, 10, 10, 20)).toEqual(a);
  });

  it('keeps every dash within the line and covers ~dash/(dash+gap) of it', () => {
    const segments = dashSegments(line, 12, 9, 7);
    let covered = 0;
    for (const [from, to] of segments) {
      expect(from.x).toBeGreaterThanOrEqual(0);
      expect(to.x).toBeLessThanOrEqual(100);
      covered += Math.abs(to.x - from.x);
    }
    expect(covered).toBeGreaterThan(100 * (12 / 21) - 12);
    expect(covered).toBeLessThan(100 * (12 / 21) + 12);
  });
});

describe('flow segments (the taper’s arc positions)', () => {
  const line = [
    { x: 0, y: 0 },
    { x: 100, y: 0 },
  ];

  it('reports each piece’s normalized position, source 0 → destination 1', () => {
    const solid = flowSegments(
      [
        { x: 0, y: 0 },
        { x: 50, y: 0 },
        { x: 100, y: 0 },
      ],
      0,
      0,
      0,
    );
    expect(solid.map((s) => s.t)).toEqual([0.25, 0.75]);
    const dashed = flowSegments(line, 10, 10, 0);
    // Monotonic and bounded — this is what makes a lone visible dash state the
    // relationship's direction without either endpoint in view.
    expect(dashed.every((s) => s.t >= 0 && s.t <= 1)).toBe(true);
    for (let i = 1; i < dashed.length; i += 1) {
      expect(dashed[i]!.t).toBeGreaterThan(dashed[i - 1]!.t);
    }
  });

  it('is the same cut dashSegments makes', () => {
    expect(flowSegments(line, 12, 9, 5).map((s) => [s.from, s.to])).toEqual(
      dashSegments(line, 12, 9, 5),
    );
  });
});

describe('endpoint geometry', () => {
  const rect = { x: 100, y: 100, w: 60, h: 100 };

  it('puts a source cap on the rect edge toward the destination', () => {
    expect(rectEdgePoint(rect, { x: 1000, y: 150 })).toEqual({ x: 160, y: 150 });
    expect(rectEdgePoint(rect, { x: 130, y: -100 })).toEqual({ x: 130, y: 100 });
    // Degenerate: a destination at the rect's own centre has no direction.
    expect(rectEdgePoint(rect, { x: 130, y: 150 })).toEqual({ x: 130, y: 150 });
  });

  it('trims a path so it stops exactly at its destination cap', () => {
    const points = pathCurve({ x: 0, y: 0 }, { x: 300, y: 0 });
    const trimmed = trimEnd(points, 20);
    const tip = trimmed[trimmed.length - 1]!;
    // Sub-hundredth: the trim interpolates along the sampled chord, so it lands
    // on the cap to well inside a pixel.
    expect(Math.hypot(300 - tip.x, 0 - tip.y)).toBeCloseTo(20, 2);
    // The trimmed end tangent is the cap's arrival tangent.
    expect(endTangent(trimmed)).toBeCloseTo(endTangent(points), 1);
  });

  it('retracts a path from its source, keeping the destination end', () => {
    const points = pathCurve({ x: 0, y: 0 }, { x: 300, y: 0 });
    const retracted = trimStart(points, 0.5);
    expect(retracted[retracted.length - 1]).toEqual(points[points.length - 1]);
    expect(polylineLength(retracted)).toBeCloseTo(polylineLength(points) / 2, 4);
  });

  it('clamps an occluded endpoint onto its container’s boundary (§10.3)', () => {
    const container = { x: 0, y: 0, w: 100, h: 100 };
    expect(clampToRect(container, { x: 200, y: 50 })).toEqual({ x: 100, y: 50 });
    expect(clampToRect(container, { x: -5, y: -5 })).toEqual({ x: 0, y: 0 });
    // A point INSIDE still lands on an edge — never in the middle of the rail.
    expect(clampToRect(container, { x: 90, y: 50 })).toEqual({ x: 100, y: 50 });
  });

  it('draws a 90° crest arc as chords on the crest’s own radius', () => {
    const chords = arcChords({ x: 0, y: 0 }, 10, 0, Math.PI / 2, 5);
    expect(chords).toHaveLength(5);
    for (const [a, b] of chords) {
      expect(Math.hypot(a.x, a.y)).toBeCloseTo(10, 6);
      expect(Math.hypot(b.x, b.y)).toBeCloseTo(10, 6);
    }
    // The sweep is a quarter turn, centred on the arrival tangent.
    const start = chords[0]![0];
    const end = chords[4]![1];
    const between = Math.abs(Math.atan2(end.y, end.x) - Math.atan2(start.y, start.x));
    expect(between).toBeCloseTo(Math.PI / 2, 6);
  });

  it('opens a zone bracket toward the arriving path', () => {
    const [spine, armA, armB] = bracketArms({ x: 0, y: 0 }, 0, 12, 28);
    // The spine crosses the path; the arms run back along it.
    expect(spine![0].x).toBeCloseTo(0, 6);
    expect(Math.hypot(spine![1].x - spine![0].x, spine![1].y - spine![0].y)).toBeCloseTo(28, 6);
    expect(armA![1].x).toBeCloseTo(-12, 6);
    expect(armB![1].x).toBeCloseTo(-12, 6);
  });

  it('routes an elbow bracket through two axis-aligned corners', () => {
    const strokes = elbowPath({ x: 0, y: 0 }, { x: 40, y: 30 });
    expect(strokes).toHaveLength(4);
    for (const [a, b] of strokes) {
      expect(a.x === b.x || a.y === b.y).toBe(true);
    }
  });
});

describe('arrowhead', () => {
  it('draws two wings from the terminus along the end tangent', () => {
    const points = pathCurve({ x: 0, y: 100 }, { x: 200, y: 100 });
    const wings = arrowHead(points);
    expect(wings).toHaveLength(2);
    const tip = points[points.length - 1]!;
    expect(wings[0]![0]).toEqual(tip);
    expect(wings[1]![0]).toEqual(tip);
    // Wings trail behind the tip.
    expect(wings[0]![1].x).toBeLessThan(tip.x);
    expect(wings[1]![1].x).toBeLessThan(tip.x);
  });
});

describe('burst particles (deterministic, never random)', () => {
  it('produces identical seeds for identical counts', () => {
    expect(burstParticles(12)).toEqual(burstParticles(12));
  });

  it('spreads directions and varies speed/size by index', () => {
    const particles = burstParticles(8);
    const angles = new Set(particles.map((p) => p.angle.toFixed(4)));
    expect(angles.size).toBe(8);
    const speeds = new Set(particles.map((p) => p.speed));
    expect(speeds.size).toBeGreaterThan(1);
  });
});
