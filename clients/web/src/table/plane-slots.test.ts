import { describe, expect, it } from 'vitest';
import { rectsOverlap } from './scene';
import { PLANE } from './plane';
import {
  DESKTOP,
  PHONE,
  WIDE16,
  ULTRAWIDE,
  TABLET,
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
      perms: [
        { id: 'p1_atk', controller: 'p1', attacking: true, attacking_player: 'p3' },
        { id: 'p1_atk_2', controller: 'p1', attacking: true, attacking_player: 'p4' },
      ],
    });
    const plane = stage(view, PHONE);
    // The first attacked seat is auto-focus-eligible and takes the drawn board;
    // the second stays a tile and wears its ring all the same — combat against
    // any seat is staged regardless of focus.
    expect(plane.focusSeat).toBe('p3');
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

describe('stagePlane ultrawide surplus-width policy (issue #500, layout-model §Hand-offs)', () => {
  it('spends surplus width on the wings, not the corridor', () => {
    const view = seatTable({ opponents: 5, active: 'p2' });
    const wide = stage(view, WIDE16);
    const ultra = stage(view, ULTRAWIDE);
    // The focused far side and the center corridor are capped at the corridor's
    // max aspect — the 21:9 surplus never widens them past the 16:9 baseline.
    expect(ultra.corridor.w).toBeCloseTo(wide.corridor.w, 5);
    expect(ultra.farSide!.rect.w).toBeCloseTo(wide.farSide!.rect.w, 5);
    // The capped central column stays centered in the ultrawide plane.
    expect(ultra.corridor.x + ultra.corridor.w / 2).toBeCloseTo(ULTRAWIDE.width / 2, 5);
    // Every wing is wider at ultrawide — the surplus lands on the wings first.
    expect(ultra.wings).toHaveLength(wide.wings.length);
    for (let i = 0; i < ultra.wings.length; i += 1) {
      expect(ultra.wings[i]!.rect.w).toBeGreaterThan(wide.wings[i]!.rect.w);
    }
  });

  it('a duel keeps the full-width far side at ultrawide (no wings to fund)', () => {
    const plane = stage(seatTable({ opponents: 1, perms: bears('p2', 2) }), ULTRAWIDE);
    expect(plane.wings).toHaveLength(0);
    // Far side spans the same width as the receiver band — uncapped, full width.
    expect(plane.farSide!.rect.w).toBeCloseTo(plane.receiver!.rect.w, 5);
  });
});

describe('stagePlane tablet-landscape floor (issue #500, layout-model §Hand-offs)', () => {
  it('holds full desktop staging at the 1180×820 floor', () => {
    const plane = stage(seatTable({ opponents: 3, active: 'p2' }), TABLET);
    expect(plane.compact).toBe(false);
    expect(plane.tiles).toHaveLength(0);
    expect(plane.farSide?.seat).toBe('p2');
    expect(plane.wings.map((w) => w.seat)).toEqual(['p3', 'p4']);
    // The desktop wing rung (a drawn board), not the compact change-of-kind.
    expect(plane.wings.every((w) => w.rung < 4)).toBe(true);
  });

  it('engages the compact branch below the floor width', () => {
    const belowFloor = { width: PLANE.compactFloorWidth - 80, height: 820 };
    const plane = stage(seatTable({ opponents: 3, active: 'p2' }), belowFloor);
    expect(plane.compact).toBe(true);
    expect(plane.wings).toHaveLength(0);
    expect(plane.tiles.map((t) => t.seat)).toEqual(['p3', 'p4']);
  });
});

describe('stagePlane five-player 2+1 wing split (issue #500 — validated as-is)', () => {
  it('stages 2 left / 1 right digest wings that fit above the receiver, no overlap', () => {
    const plane = stage(
      seatTable({
        opponents: 4,
        active: 'p2',
        perms: [...menagerie('p3', 6), ...menagerie('p5', 5)],
      }),
    );
    expect(plane.wings.map((w) => [w.side, w.rank])).toEqual([
      ['left', 0],
      ['right', 0],
      ['left', 1],
    ]);
    expect(plane.wings.every((w) => w.digest !== undefined)).toBe(true);
    // The two stacked left wings never overlap each other…
    const left = plane.wings.filter((w) => w.side === 'left');
    expect(rectsOverlap(left[0]!.rect, left[1]!.rect)).toBe(false);
    // …and every wing clears the receiver band with a live crest.
    for (const wing of plane.wings) {
      expect(wing.rect.y + wing.rect.h).toBeLessThanOrEqual(plane.receiver!.rect.y);
      expect(wing.crest.w).toBeGreaterThanOrEqual(PLANE.minHit);
      expect(wing.crest.h).toBeGreaterThanOrEqual(PLANE.minHit);
    }
  });
});

describe('stagePlane standing invariants at every count and geometry (issue #500)', () => {
  const geometries = [
    ['desktop', DESKTOP],
    ['wide16', WIDE16],
    ['ultrawide', ULTRAWIDE],
    ['tablet', TABLET],
  ] as const;
  for (const [label, viewport] of geometries) {
    for (const opponents of [1, 2, 3, 4, 5]) {
      it(`stages ${opponents + 1} players at ${label}: no overlap, crests + piles ≥ 44 px`, () => {
        const perms = [...menagerie('p1', 6), ...menagerie('p2', 8), ...bears('p3', 10)];
        const plane = stage(seatTable({ opponents, active: 'p2', perms }), viewport);
        // No focus branch to compact here — every landscape geometry is desktop.
        expect(plane.compact).toBe(false);
        const regions = regionsOf(plane);
        for (let i = 0; i < regions.length; i += 1) {
          for (let j = i + 1; j < regions.length; j += 1) {
            expect(rectsOverlap(regions[i]!.rect, regions[j]!.rect)).toBe(false);
          }
        }
        for (const region of regions) {
          expect(region.crest.w).toBeGreaterThanOrEqual(PLANE.minHit);
          expect(region.crest.h).toBeGreaterThanOrEqual(PLANE.minHit);
          expect(region.piles.w).toBeGreaterThanOrEqual(PLANE.minHit);
          expect(region.piles.h).toBeGreaterThanOrEqual(PLANE.minHit);
          for (const render of region.renders) {
            expect(render.hitRect.w).toBeGreaterThanOrEqual(PLANE.minHit);
            expect(render.hitRect.h).toBeGreaterThanOrEqual(PLANE.minHit);
          }
        }
        for (const rect of allPlaneRects(plane)) {
          expect(rectsOverlap(rect, plane.corridor)).toBe(false);
        }
      });
    }
  }
});

/*
 * The staging box (issue #534, ADR 0032).
 *
 * Under the contextual shell the plane spans the whole viewport — the arena has
 * to remain visible behind the controls — but its SLOTS may not be carved under
 * the hand fan or the lower-right control cluster. `PlaneViewport.safe` is that
 * separation: coordinate space stays the viewport, staging happens inside the
 * box. These tests pin the property that makes it safe to rely on.
 */
describe('staging box — slots are carved inside the chrome-free rect', () => {
  /** A 1280×800 desktop with the #534 chrome standing on it. */
  const SAFE = { x: 0, y: 0, w: DESKTOP.width - 324, h: DESKTOP.height - 210 };
  const inset = { ...DESKTOP, safe: SAFE };

  /** Whether `inner` lies entirely inside `outer` (a 0.5px tolerance for rounding). */
  const within = (outer: typeof SAFE, inner: { x: number; y: number; w: number; h: number }) =>
    inner.x >= outer.x - 0.5 &&
    inner.y >= outer.y - 0.5 &&
    inner.x + inner.w <= outer.x + outer.w + 0.5 &&
    inner.y + inner.h <= outer.y + outer.h + 0.5;

  it('omitting the box stages exactly as it did before #534', () => {
    // The compatibility guarantee every existing caller and fixture rides on.
    const before = stage(seatTable({ opponents: 3, perms: menagerie('p2', 6) }));
    const explicit = stage(seatTable({ opponents: 3, perms: menagerie('p2', 6) }), {
      ...DESKTOP,
      safe: { x: 0, y: 0, w: DESKTOP.width, h: DESKTOP.height },
    });
    expect(explicit.receiver!.rect).toEqual(before.receiver!.rect);
    expect(explicit.farSide!.rect).toEqual(before.farSide!.rect);
    expect(explicit.corridor).toEqual(before.corridor);
  });

  it('keeps the receiver band and the far side clear of the chrome', () => {
    // The failure this prevents: the receiver's band resolving against the raw
    // viewport and landing underneath the hand fan, so the player's own board is
    // behind their own cards.
    const plane = stage(seatTable({ opponents: 3, perms: menagerie('p2', 6) }), inset);
    expect(within(SAFE, plane.receiver!.rect)).toBe(true);
    expect(within(SAFE, plane.farSide!.rect)).toBe(true);
    expect(plane.receiver!.rect.y + plane.receiver!.rect.h).toBeCloseTo(SAFE.h);
  });

  it('measures the wing bleed from the box edge, not the plane edge', () => {
    // A wing tucks a constant FRACTION OF ITSELF offstage. Measured from the
    // plane edge instead, the right-hand wing would sit under the cluster and
    // the bleed would grow as chrome takes more width.
    const plane = stage(seatTable({ opponents: 4 }), inset);
    const right = plane.wings.filter((wing) => wing.side === 'right');
    expect(right.length).toBeGreaterThan(0);
    for (const wing of right) {
      const overhang = wing.rect.x + wing.rect.w - SAFE.w;
      expect(overhang).toBeCloseTo(wing.rect.w * PLANE.wing.bleed);
    }
    for (const wing of plane.wings.filter((w) => w.side === 'left')) {
      expect(wing.rect.x).toBeCloseTo(-wing.rect.w * PLANE.wing.bleed);
    }
  });

  it('keeps every wing crest inside the box, at every populated count', () => {
    // Crests are the selection surface for player-targeting and attack
    // declaration (`layout-model.md`), so a crest under the control cluster is
    // an unpickable target, not a cosmetic problem. The boards bleed; the crests
    // may not.
    for (const opponents of [2, 3, 4, 5]) {
      const plane = stage(seatTable({ opponents }), inset);
      for (const region of regionsOf(plane)) {
        expect(within(SAFE, region.crest), `${opponents} opponents`).toBe(true);
      }
    }
  });

  it('insets the compact branch tiles and receiver too', () => {
    const phoneSafe = { x: 0, y: 0, w: PHONE.width, h: PHONE.height - 180 };
    const plane = stage(seatTable({ opponents: 3 }), { ...PHONE, safe: phoneSafe });
    expect(within(phoneSafe, plane.receiver!.rect)).toBe(true);
    expect(plane.tiles.length).toBeGreaterThan(0);
    for (const tile of plane.tiles) {
      expect(within(phoneSafe, tile.rect)).toBe(true);
    }
  });

  it('honours a box that is offset, not just smaller', () => {
    // Nothing in #534 needs a left/top offset today, but a fraction resolved
    // against the box's size and then positioned from the PLANE's origin is a
    // bug that only appears once one is used. Pin it now.
    const offset = { x: 40, y: 24, w: DESKTOP.width - 364, h: DESKTOP.height - 234 };
    const plane = stage(seatTable({ opponents: 3 }), { ...DESKTOP, safe: offset });
    expect(within(offset, plane.receiver!.rect)).toBe(true);
    expect(within(offset, plane.farSide!.rect)).toBe(true);
    expect(plane.receiver!.rect.y + plane.receiver!.rect.h).toBeCloseTo(offset.y + offset.h);
  });
});
