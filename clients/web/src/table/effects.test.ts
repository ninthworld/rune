/**
 * Effects geometry and vocabulary policy (issue #482): pure, deterministic
 * pieces of the passive effects layer — anchors, the path curve, dash-crawl
 * segmentation, arrowheads, and the seeded (never random) burst particles.
 */
import { describe, expect, it } from 'vitest';
import {
  anchorCenter,
  arrowHead,
  burstParticles,
  dashSegments,
  pathCurve,
  rectCenter,
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
