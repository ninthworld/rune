import { describe, expect, it } from 'vitest';
import { rectsOverlap } from './layout';
import { PLANE } from './plane';
import {
  DESKTOP,
  PHONE,
  seatTable,
  bears,
  menagerie,
  stage,
  regionsOf,
  allPlaneRects,
} from './plane.fixture';

describe('stagePlane fixed slots per player count (issue #478, layout-model §Staging)', () => {
  it('stages a duel as receiver band + full-width far side, no wings, no focus', () => {
    const plane = stage(seatTable({ opponents: 1, perms: bears('p2', 2) }));
    expect(plane.receiver?.seat).toBe('p1');
    expect(plane.farSide?.seat).toBe('p2');
    expect(plane.wings).toHaveLength(0);
    // No focus concept exists at two players.
    expect(plane.focusSeat).toBeUndefined();
    expect(plane.farSide?.focused).toBe(false);
    // The opponent owns the full far side — same width as the receiver's band.
    expect(plane.farSide?.rect.w).toBeCloseTo(plane.receiver!.rect.w);
  });

  it('keeps the receiver on the full-width bottom band at every count', () => {
    for (const opponents of [1, 2, 3, 4, 5]) {
      const plane = stage(seatTable({ opponents }));
      const receiver = plane.receiver!;
      // Bottom third (±) of the plane, below every other region.
      expect(receiver.rect.y + receiver.rect.h).toBeCloseTo(DESKTOP.height);
      expect(receiver.rect.h).toBeCloseTo(DESKTOP.height * PLANE.receiver.h);
      for (const other of [plane.farSide!, ...plane.wings]) {
        expect(other.rect.y + other.rect.h).toBeLessThanOrEqual(receiver.rect.y);
      }
    }
  });

  it('stages 3 players as focused far side + one full-board wing', () => {
    const plane = stage(seatTable({ opponents: 2, active: 'p2' }));
    expect(plane.farSide?.seat).toBe('p2');
    expect(plane.wings.map((w) => w.seat)).toEqual(['p3']);
    // One wing per side is the larger, full-board wing — not digest-baseline.
    expect(plane.wings[0]?.rung).toBeLessThan(4);
    expect(plane.wings[0]?.digest).toBeUndefined();
  });

  it('stages 4 players as focused far side + one wing per side', () => {
    const plane = stage(seatTable({ opponents: 3, active: 'p2' }));
    expect(plane.farSide?.seat).toBe('p2');
    expect(plane.wings.map((w) => w.side)).toEqual(['left', 'right']);
    expect(plane.wings.every((w) => w.rank === 0)).toBe(true);
    expect(plane.wings.every((w) => w.rung < 4)).toBe(true);
  });

  it('stages 5 players as 2 wings left, 1 right, at the digest rung', () => {
    const plane = stage(seatTable({ opponents: 4, active: 'p2' }));
    expect(plane.wings.map((w) => w.side)).toEqual(['left', 'right', 'left']);
    expect(plane.wings.map((w) => w.rank)).toEqual([0, 0, 1]);
    // Two-per-side staging is the digest wing rung (layout-model table).
    expect(plane.wings.every((w) => w.rung === 4)).toBe(true);
  });

  it('stages 6 players as two wings per side at the digest rung', () => {
    const plane = stage(seatTable({ opponents: 5, active: 'p2' }));
    expect(plane.wings.map((w) => w.side)).toEqual(['left', 'right', 'left', 'right']);
    expect(plane.wings.map((w) => w.rank)).toEqual([0, 0, 1, 1]);
    expect(plane.wings.every((w) => w.rung === 4)).toBe(true);
    expect(plane.wings.every((w) => w.digest !== undefined)).toBe(true);
  });

  it('never overlaps one seat region with another (by construction)', () => {
    for (const opponents of [1, 2, 3, 4, 5]) {
      const regions = regionsOf(stage(seatTable({ opponents })));
      for (let i = 0; i < regions.length; i += 1) {
        for (let j = i + 1; j < regions.length; j += 1) {
          expect(rectsOverlap(regions[i]!.rect, regions[j]!.rect)).toBe(false);
        }
      }
    }
  });
});

describe('stagePlane seat order stability (issue #478)', () => {
  it('stages wings in seat order, not projection order', () => {
    const view = seatTable({ opponents: 3, seatOrder: ['p1', 'p4', 'p3', 'p2'], active: 'p1' });
    const plane = stage(view);
    // Default focus on the receiver's turn: next seat in turn order — p4.
    expect(plane.farSide?.seat).toBe('p4');
    expect(plane.wings.map((w) => w.seat)).toEqual(['p3', 'p2']);
  });

  it("never reshuffles a seat's wing slot because of game state", () => {
    const before = stage(seatTable({ opponents: 4, active: 'p2' }));
    // The same table later: boards grew, life changed — the staging is stable.
    const later = seatTable({
      opponents: 4,
      active: 'p2',
      perms: [...menagerie('p3', 6), ...bears('p5', 9)],
    });
    const after = stage(later);
    expect(after.farSide?.seat).toBe(before.farSide?.seat);
    expect(after.wings.map((w) => w.seat)).toEqual(before.wings.map((w) => w.seat));
    expect(after.wings.map((w) => w.side)).toEqual(before.wings.map((w) => w.side));
    // A bystander mounting mid-game derives the identical staging (pure data).
    expect(stage(later)).toEqual(after);
  });

  it('keeps an eliminated seat staged in its slot, zones browsable', () => {
    const plane = stage(seatTable({ opponents: 3, active: 'p2', eliminated: ['p4'] }));
    const wing = plane.wings.find((w) => w.seat === 'p4');
    expect(wing).toBeDefined();
    expect(wing?.eliminated).toBe(true);
    // Public piles stay browsable on the eliminated seat's slot.
    expect(wing?.zones.library).toBe(60);
    expect(wing?.crest.w).toBeGreaterThanOrEqual(PLANE.minHit);
  });

  it('stages every seat as an opponent when the receiver is unknown (legacy)', () => {
    const plane = stage(seatTable({ opponents: 3, you: '' }));
    expect(plane.receiver).toBeUndefined();
    expect(plane.farSide).toBeDefined();
    expect(1 + plane.wings.length).toBe(3);
  });
});

describe('stagePlane center corridor (issue #478, layout-model §The plane)', () => {
  it('keeps the corridor clear at every player count, even on busy boards', () => {
    for (const opponents of [1, 2, 3, 4, 5]) {
      const perms = [
        ...menagerie('p1', 8),
        ...menagerie('p2', 10),
        ...bears('p2', 12, { prefix: 'fold' }),
        ...(opponents >= 2 ? menagerie('p3', 14) : []),
      ];
      const plane = stage(seatTable({ opponents, perms }));
      expect(plane.corridor.w).toBeGreaterThan(DESKTOP.width * 0.3);
      expect(plane.corridor.h).toBeGreaterThan(0);
      for (const rect of allPlaneRects(plane)) {
        expect(rectsOverlap(rect, plane.corridor)).toBe(false);
      }
    }
  });

  it('spans the corridor between the far side and the receiver band', () => {
    const plane = stage(seatTable({ opponents: 3 }));
    const far = plane.farSide!.rect;
    const receiver = plane.receiver!.rect;
    expect(plane.corridor.y).toBeCloseTo(far.y + far.h);
    expect(plane.corridor.y + plane.corridor.h).toBeCloseTo(receiver.y);
  });
});

describe('stagePlane interaction floors (presentation-budgets §Accessibility)', () => {
  it('keeps every crest cluster and render hotspot at ≥ 44 px', () => {
    const view = seatTable({
      opponents: 5,
      active: 'p2',
      perms: [...menagerie('p2', 10), ...bears('p3', 30), ...menagerie('p1', 6)],
    });
    const plane = stage(view);
    for (const region of regionsOf(plane)) {
      expect(region.crest.w).toBeGreaterThanOrEqual(PLANE.minHit);
      expect(region.crest.h).toBeGreaterThanOrEqual(PLANE.minHit);
      expect(region.piles.w).toBeGreaterThanOrEqual(PLANE.minHit);
      for (const render of region.renders) {
        expect(render.hitRect.w).toBeGreaterThanOrEqual(PLANE.minHit);
        expect(render.hitRect.h).toBeGreaterThanOrEqual(PLANE.minHit);
      }
    }
  });
});

describe('stagePlane compact change-of-kind (rung 5, phone portrait)', () => {
  it('collapses peripheral opponents to ≥ 44 px summary tiles at 3+ players', () => {
    const plane = stage(seatTable({ opponents: 3, active: 'p2' }), PHONE);
    expect(plane.compact).toBe(true);
    expect(plane.farSide?.seat).toBe('p2');
    expect(plane.wings).toHaveLength(0);
    expect(plane.tiles.map((t) => t.seat)).toEqual(['p3', 'p4']);
    for (const tile of plane.tiles) {
      expect(tile.rect.h).toBeGreaterThanOrEqual(PLANE.minHit);
      expect(tile.life).toBe(40);
      expect(tile.handCount).toBe(3);
    }
    // The receiver keeps the bottom anatomy; the focused board stays drawn.
    expect(plane.receiver?.rect.y).toBeGreaterThan(plane.farSide!.rect.y);
    expect(plane.farSide?.renders).toBeDefined();
  });

  it('marks an attacked tile seat (off-focus activity is never silent)', () => {
    const view = seatTable({
      opponents: 3,
      active: 'p1',
      perms: [{ id: 'p1_atk', controller: 'p1', attacking: true, attacking_player: 'p4' }],
    });
    const plane = stage(view, PHONE);
    expect(plane.tiles.find((t) => t.seat === 'p4')?.attacked).toBe(true);
  });

  it('still draws both boards in full on a phone duel (tiles need 2+ opponents)', () => {
    const plane = stage(seatTable({ opponents: 1, perms: bears('p2', 3) }), PHONE);
    expect(plane.compact).toBe(false);
    expect(plane.tiles).toHaveLength(0);
    expect(plane.farSide?.renders.length).toBeGreaterThan(0);
  });

  it('keeps the corridor beside the tile column clear', () => {
    const plane = stage(seatTable({ opponents: 3, active: 'p2' }), PHONE);
    expect(plane.corridor.w).toBeGreaterThan(0);
    for (const rect of allPlaneRects(plane)) {
      expect(rectsOverlap(rect, plane.corridor)).toBe(false);
    }
  });
});
