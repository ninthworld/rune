import { describe, expect, it } from 'vitest';
import {
  doubledStroke,
  linkAlpha,
  linkTouches,
  positionLinks,
  selectVisibleLinks,
} from './combatLinks';
import type { CombatLink } from './scene';
import { COMBAT_LINK } from '../tokens';

const link = (blocker: string, attacker: string): CombatLink => ({ blocker, attacker });

describe('combatLinks policy (issue #339)', () => {
  it('draws every link when nothing is isolated', () => {
    const links = [link('b1', 'a1'), link('b2', 'a2')];
    expect(selectVisibleLinks(links, null)).toEqual(links);
  });

  it('isolates one participant’s links on a crowded board rather than drawing all', () => {
    const links = [
      link('b1', 'a1'),
      link('b2', 'a1'),
      link('b3', 'a2'),
      link('b4', 'a3'),
      link('b5', 'a4'),
      link('b6', 'a5'),
      link('b7', 'a6'), // 7 links — crowded
    ];
    // Focusing the attacker a1 keeps only the two blockers assigned to it.
    expect(selectVisibleLinks(links, 'a1')).toEqual([link('b1', 'a1'), link('b2', 'a1')]);
    // Focusing a blocker keeps just its one link.
    expect(selectVisibleLinks(links, 'b3')).toEqual([link('b3', 'a2')]);
  });

  it('recognizes a link touching an id as either blocker or attacker', () => {
    expect(linkTouches(link('b1', 'a1'), 'b1')).toBe(true);
    expect(linkTouches(link('b1', 'a1'), 'a1')).toBe(true);
    expect(linkTouches(link('b1', 'a1'), 'other')).toBe(false);
  });

  it('calms link emphasis on a crowded board, full otherwise or when isolated', () => {
    expect(linkAlpha(2, null)).toBe(COMBAT_LINK.alpha);
    expect(linkAlpha(COMBAT_LINK.crowdedThreshold + 1, null)).toBe(COMBAT_LINK.crowdedAlpha);
    // Isolating a participant restores full emphasis even on a crowded board.
    expect(linkAlpha(COMBAT_LINK.crowdedThreshold + 1, 'a1')).toBe(COMBAT_LINK.alpha);
  });
});

describe('combatLinks geometry (issue #339)', () => {
  it('resolves endpoints from the current centers and tracks position changes', () => {
    const links = [link('b1', 'a1')];
    let centers: Record<string, { x: number; y: number }> = {
      b1: { x: 0, y: 0 },
      a1: { x: 100, y: 0 },
    };
    const positioned = positionLinks(links, (id) => centers[id]);
    expect(positioned).toHaveLength(1);
    expect(positioned[0].from).toEqual({ x: 0, y: 0 });
    expect(positioned[0].to).toEqual({ x: 100, y: 0 });

    // A view-diff animation moved the attacker: the endpoint tracks it (issue #334).
    centers = { b1: { x: 0, y: 0 }, a1: { x: 100, y: 50 } };
    expect(positionLinks(links, (id) => centers[id])[0].to).toEqual({ x: 100, y: 50 });
  });

  it('drops a link whose blocker or attacker cannot be located (left play)', () => {
    const links = [link('b1', 'gone')];
    const positioned = positionLinks(links, (id) => (id === 'b1' ? { x: 0, y: 0 } : undefined));
    expect(positioned).toHaveLength(0);
  });

  it('offsets the doubled stroke perpendicular to the link direction', () => {
    // A horizontal link: the two strokes separate vertically by the configured gap.
    const [top, bottom] = doubledStroke({ x: 0, y: 0 }, { x: 10, y: 0 });
    expect(top[0].y - bottom[0].y).toBeCloseTo(COMBAT_LINK.gap);
    // Both strokes still run horizontally (same y at both ends of each).
    expect(top[0].y).toBeCloseTo(top[1].y);
    expect(bottom[0].y).toBeCloseTo(bottom[1].y);
  });

  it('does not divide by zero for a degenerate coincident link', () => {
    const strokes = doubledStroke({ x: 5, y: 5 }, { x: 5, y: 5 });
    expect(strokes).toHaveLength(2);
    expect(Number.isNaN(strokes[0][0].x)).toBe(false);
  });
});
