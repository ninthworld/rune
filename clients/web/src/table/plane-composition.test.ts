/**
 * The baseline arena composition (issue #531) — the successor of the panel-band
 * staging, held to the two specifications that bound it:
 *
 * - `docs/design/environment-system.md` §2.2/§3.3 — **every** staged rect must
 *   live inside the seat envelope (Zone A ∪ Zone B). `zones.test.ts` already
 *   asserts that for the four rects `carveSlots` produces; this file extends the
 *   same predicate to the rects `stagePlane` *attaches* — crest clusters and
 *   zone racks — which is exactly where the receiver-crest collision reported
 *   with #530 lived.
 * - `docs/design/layout-model.md` — the fixed slot groups, the focus model, and
 *   the degradation ladder, at 2–6 seats and at every reference viewport.
 *
 * What jsdom cannot show, and what this file therefore does not claim: nothing
 * here proves a rendered pixel, a real layout, a contrast reading, or that the
 * composition *looks* like the baseline. These are assertions about the declared
 * geometry contract. Whether standard 4p at 1680×945 reads as the baseline at a
 * glance is the maintainer's call in a real browser (issue #531's closure gate).
 */
import { describe, expect, it } from 'vitest';
import { cellSize, rectsOverlap, splayClearance, tabClearance, type Rect } from './scene';
import { cardVisualSignature } from '../card/cardFactory';
import { BATTLEFIELD_TIERS, CARD_BOX, faceFootprint } from '../card/dom';
import { planeDisplayData } from './planeDisplayData';
import { cardFaceRenderer } from './planeFaceRenderer';
import { PlaneReconciler } from './planeReconciler';
import { SCENE_DOM_CEILING } from './live/presentationMode';
import { PLANE, withinEnvelope, type StagedPlane } from './plane';
import {
  ENV_AMBIENT_SPACE,
  ENV_ZONES,
  inPropPocket,
  overlapsRect,
  withinSeatEnvelope,
} from './environment/zones';
import { toFractionRect } from './environment/planeOccupancy';
import {
  DESKTOP,
  TABLET,
  ULTRAWIDE,
  WIDE16,
  allPlaneRects,
  basics,
  bears,
  clusterRects,
  menagerie,
  regionsOf,
  seatTable,
  stage,
} from './plane.fixture';

/** The landscape geometries the composition is proven across. */
const VIEWPORTS = [
  ['desktop 1280×800', DESKTOP],
  ['baseline 1680×945', { width: 1680, height: 945 }],
  ['16:9 1920×1080', WIDE16],
  ['ultrawide 2560×1080', ULTRAWIDE],
  ['tablet floor 1180×820', TABLET],
] as const;

const SEAT_COUNTS = [1, 2, 3, 4, 5] as const;

/** Every rect `stagePlane` attaches to a region, in canvas fractions. */
function attachedRects(plane: StagedPlane): { label: string; rect: Rect }[] {
  const rects: { label: string; rect: Rect }[] = [];
  for (const region of regionsOf(plane)) {
    rects.push({ label: `crest:${region.seat}`, rect: region.crest });
    rects.push({ label: `rack:${region.seat}`, rect: region.piles });
    // The whole identity cluster, not just its medallion (issue #532): the
    // nameplate runs outboard and the status rail arcs above the rim, so both
    // can leave the envelope in ways the crest rect alone would never show.
    for (const [i, rect] of clusterRects(region.cluster).entries()) {
      rects.push({ label: `cluster:${region.seat}:${i}`, rect });
    }
    for (const slot of region.rack.slots) {
      rects.push({ label: `zone:${region.seat}:${slot.zone}`, rect: slot.hitRect });
    }
  }
  return rects;
}

describe('battlefield composition — the seat envelope holds for attached chrome too', () => {
  for (const [label, viewport] of VIEWPORTS) {
    for (const opponents of SEAT_COUNTS) {
      it(`keeps every crest and rack inside Zone A ∪ Zone B at ${opponents + 1} seats on ${label}`, () => {
        const plane = stage(seatTable({ opponents, active: 'p2' }), viewport);
        for (const { label: what, rect } of attachedRects(plane)) {
          const fraction = toFractionRect(rect, viewport);
          // Reported as an object so a failure names the offending rect rather
          // than just printing `false`.
          expect({ what, inside: withinSeatEnvelope(fraction) }).toEqual({ what, inside: true });
          // And the plane's own copy of the predicate agrees with the
          // environment's, so the two can never drift apart.
          expect(withinEnvelope(rect, viewport)).toBe(true);
        }
      });
    }
  }

  it('agrees byte-for-byte with the environment’s Zone A ∪ Zone B constants', () => {
    // `plane/metrics.ts` transcribes the envelope rather than importing it (the
    // dependency runs environment → plane). This is the pin that keeps the
    // transcription honest.
    expect(PLANE.envelope.coreX).toBe(ENV_ZONES.focalCore.x);
    expect(PLANE.envelope.flankTop).toBe(ENV_ZONES.seatFlanks[0]!.y);
    expect(PLANE.envelope.flankBottom).toBeCloseTo(
      ENV_ZONES.seatFlanks[0]!.y + ENV_ZONES.seatFlanks[0]!.h,
      10,
    );
  });

  it('keeps every staged rect out of the four prop pockets', () => {
    // Zone C is the whole budget for illustrated incident (environment-system
    // §2.2); layout may not draw there without amending §2.2 (§3.3).
    for (const [, viewport] of VIEWPORTS) {
      for (const opponents of SEAT_COUNTS) {
        const plane = stage(seatTable({ opponents, active: 'p2' }), viewport);
        for (const rect of allPlaneRects(plane)) {
          const fraction = toFractionRect(rect, viewport);
          for (const pocket of ENV_ZONES.propPockets) {
            expect(overlapsRect(fraction, pocket)).toBe(false);
          }
        }
      }
    }
  });

  it('never stages anything in the uncontested ambient reservation (§6.3)', () => {
    // The bottom-left pocket §6.3 declares "never contested by any seat at any
    // count" — it also carries the in-match wordmark. Before #531 the receiver's
    // crest sat squarely inside it (x 0.051–0.082, y 0.702–0.757 at 1680×945).
    for (const [, viewport] of VIEWPORTS) {
      for (const opponents of SEAT_COUNTS) {
        const plane = stage(seatTable({ opponents, active: 'p2' }), viewport);
        for (const rect of allPlaneRects(plane)) {
          const fraction = toFractionRect(rect, viewport);
          expect(overlapsRect(fraction, ENV_AMBIENT_SPACE.uncontested)).toBe(false);
        }
      }
    }
  });

  it('stages every seat crest on the plane, not off its edges', () => {
    // The focused opponent's crest used to resolve to a negative `y` at every
    // reference viewport — the seat that can never degrade away was off-canvas.
    for (const [, viewport] of VIEWPORTS) {
      for (const opponents of SEAT_COUNTS) {
        for (const region of regionsOf(stage(seatTable({ opponents, active: 'p2' }), viewport))) {
          expect(region.crest.x).toBeGreaterThanOrEqual(0);
          expect(region.crest.y).toBeGreaterThanOrEqual(0);
          expect(region.crest.x + region.crest.w).toBeLessThanOrEqual(viewport.width);
          expect(region.crest.y + region.crest.h).toBeLessThanOrEqual(viewport.height);
          expect(region.crest.w).toBeGreaterThanOrEqual(PLANE.minHit);
          expect(region.crest.h).toBeGreaterThanOrEqual(PLANE.minHit);
        }
      }
    }
  });

  it('stages every zone rack on the plane, including a wing that bleeds offstage', () => {
    // A right-hand wing's slot runs past the plane edge by `wing.bleed`. Before
    // #531 the pile cluster was placed at the slot's inner corner and landed
    // entirely off-canvas (x 1.03–1.07 of W) — invisible and unreachable.
    for (const [, viewport] of VIEWPORTS) {
      for (const opponents of SEAT_COUNTS) {
        for (const region of regionsOf(stage(seatTable({ opponents, active: 'p2' }), viewport))) {
          for (const slot of region.rack.slots) {
            expect(slot.rect.x).toBeGreaterThanOrEqual(0);
            expect(slot.rect.x + slot.rect.w).toBeLessThanOrEqual(viewport.width);
            expect(slot.rect.y).toBeGreaterThanOrEqual(0);
            expect(slot.rect.y + slot.rect.h).toBeLessThanOrEqual(viewport.height);
          }
        }
      }
    }
  });
});

describe('battlefield composition — the baseline arena’s spatial ratios', () => {
  // The baseline is a large open plaza with the local seat in the bottom
  // foreground, the focused opponent across the top, and the peripheral seats
  // hanging at **mid height** on the flanks. These pin that arrangement; they
  // are transcription, not derivation, and a deliberate change to the
  // composition is expected to change them.

  it('leaves the focused opponent room for its own crest above its board', () => {
    // At the shipped `y = 0.02` the far side's crest resolved above the plane's
    // top edge at every reference viewport. The band's top must clear half the
    // medallion **that is actually drawn** so the seat's identity is staged, not
    // clamped into its own board.
    //
    // It used to be measured against `PLANE.crest`, a 52 px constant that was
    // smaller than every rung of the ladder and that no staging code read
    // (issue #582 §1). Measuring against the staged cluster's own `D` is the
    // point: a reservation that cannot see what it reserves for reserves
    // nothing.
    for (const [, viewport] of VIEWPORTS) {
      for (const opponents of [1, 3] as const) {
        const plane = stage(seatTable({ opponents, active: 'p2' }), viewport);
        const far = plane.farSide!;
        expect(far.rect.y).toBeGreaterThanOrEqual(far.cluster.d / 2 + 6);
        // …and the medallion group is staged whole, on the plane.
        expect(far.cluster.core.y).toBeGreaterThanOrEqual(0);
      }
    }
  });

  it('keeps the far side’s bottom edge — and so the corridor — where it was', () => {
    // The drop is a move, not a resize of the interaction area: the corridor's
    // top edge is unchanged, so nothing downstream of it shifts.
    expect(PLANE.far.y + PLANE.far.h).toBeCloseTo(0.36, 10);
    expect(PLANE.duelFar.y + PLANE.duelFar.h).toBeCloseTo(0.36, 10);
  });

  it('hangs a lone peripheral seat at mid height on its flank', () => {
    // `rune-2.5d-interface-baseline.jpg` puts the 4-player wing medallions at
    // y ≈ 0.28 — beside the arena, not pinned to the top edge.
    for (const [, viewport] of VIEWPORTS) {
      const plane = stage(seatTable({ opponents: 3, active: 'p2' }), viewport);
      expect(plane.wings).toHaveLength(2);
      for (const wing of plane.wings) {
        const centre = (wing.rect.y + wing.rect.h / 2) / viewport.height;
        expect(centre).toBeGreaterThan(0.35);
        expect(centre).toBeLessThan(0.55);
      }
    }
  });

  it('keeps the two-per-side stack spanning the flank instead, at 5–6 seats', () => {
    // Two ranks have to cover the flank band, which is what environment-system
    // §3.1 panels 4–5 describe; they keep the top anchor for that reason.
    for (const [, viewport] of VIEWPORTS) {
      const plane = stage(seatTable({ opponents: 5, active: 'p2' }), viewport);
      const left = plane.wings.filter((w) => w.side === 'left');
      expect(left).toHaveLength(2);
      const top = Math.min(...left.map((w) => w.rect.y)) / viewport.height;
      const bottom = Math.max(...left.map((w) => w.rect.y + w.rect.h)) / viewport.height;
      expect(top).toBeCloseTo(PLANE.wing.top, 6);
      expect(bottom).toBeGreaterThan(0.6);
      expect(bottom).toBeLessThanOrEqual(PLANE.envelope.flankBottom);
    }
  });

  it('leaves the local seat the bottom foreground, full width, below everything', () => {
    for (const [, viewport] of VIEWPORTS) {
      for (const opponents of SEAT_COUNTS) {
        const plane = stage(seatTable({ opponents, active: 'p2' }), viewport);
        const receiver = plane.receiver!;
        // Flush with the box's bottom edge at 3+ players, where the flank wings
        // hang in the band above it; dropped by the far side's own top margin in
        // a duel, where the two rows are the whole board and sit symmetrically
        // inside the arena (issue #582 §2).
        const margin = opponents === 1 ? viewport.height * PLANE.duelFar.y : 0;
        expect(receiver.rect.y + receiver.rect.h).toBeCloseTo(viewport.height - margin, 6);
        for (const other of [plane.farSide!, ...plane.wings]) {
          expect(other.rect.y + other.rect.h).toBeLessThanOrEqual(receiver.rect.y);
        }
        // …and the largest tier, so the foreground reads as the foreground.
        expect(receiver.surface).toBe('field');
      }
    }
  });
});

describe('battlefield composition — the open central arena', () => {
  it('keeps the corridor clear of every staged rect at every count and geometry', () => {
    for (const [, viewport] of VIEWPORTS) {
      for (const opponents of SEAT_COUNTS) {
        const plane = stage(
          seatTable({
            opponents,
            active: 'p2',
            perms: [...menagerie('p1', 8), ...menagerie('p2', 10), ...bears('p3', 14)],
          }),
          viewport,
        );
        for (const rect of allPlaneRects(plane)) {
          expect(rectsOverlap(rect, plane.corridor)).toBe(false);
        }
      }
    }
  });

  it('leaves the environment medallion inside the corridor at every count', () => {
    // The arena's identity mark sits at (50 %, 40 %) and the corridor is what
    // guarantees no card ever parks on it (environment-system §2.3).
    for (const [, viewport] of VIEWPORTS) {
      for (const opponents of SEAT_COUNTS) {
        const plane = stage(seatTable({ opponents, active: 'p2' }), viewport);
        const corridor = toFractionRect(plane.corridor, viewport);
        expect(corridor.y).toBeLessThanOrEqual(0.4);
        expect(corridor.y + corridor.h).toBeGreaterThanOrEqual(0.4);
      }
    }
  });

  /**
   * Issue #582 §2. The shipped duel put the far side at `0.09…0.36` of the
   * staging box and the receiver flush with its bottom edge, which resolves —
   * in the maintainer's capture — to the player's row near the CENTRE of the
   * arena and the opponent's jammed against the top, with the opponent's
   * creature clipped by the viewport and their hand fan cut off above it.
   */
  it('seats both duel rows symmetrically inside the arena', () => {
    for (const [label, viewport] of VIEWPORTS) {
      const plane = stage(seatTable({ opponents: 1, active: 'p2' }), viewport);
      const far = plane.farSide!.rect;
      const receiver = plane.receiver!.rect;
      // The staging box, which is the plane itself in this fixture.
      const topMargin = far.y;
      const bottomMargin = viewport.height - (receiver.y + receiver.h);
      expect(bottomMargin, `asymmetric board at ${label}`).toBeCloseTo(topMargin, 6);
      // Neither row runs off an edge of the box it is carved in.
      expect(far.y).toBeGreaterThan(0);
      expect(receiver.y + receiver.h).toBeLessThanOrEqual(viewport.height);
    }
  });

  it('reserves a legible corridor between the two duel rows', () => {
    for (const [label, viewport] of VIEWPORTS) {
      const plane = stage(
        seatTable({
          opponents: 1,
          active: 'p2',
          perms: [...menagerie('p1', 8), ...bears('p2', 8)],
        }),
        viewport,
      );
      const gap = plane.receiver!.rect.y - (plane.farSide!.rect.y + plane.farSide!.rect.h);
      // The band Declare Attackers is drawn in. Stated as a floor rather than
      // left as whatever the two rows happen not to take.
      expect(gap / viewport.height, `corridor too tight at ${label}`).toBeGreaterThanOrEqual(
        PLANE.corridorMinH,
      );
      expect(plane.corridor.h).toBeGreaterThanOrEqual(viewport.height * PLANE.corridorMinH);
    }
  });

  /**
   * Issue #582 §1. The local seat's medallion, life ring, and hand-count hex
   * were drawn over the player's own battlefield row — in the maintainer's
   * capture two of their own creatures were behind the disc, one entirely
   * unreadable — and the opponent's medallion covered their own creature.
   *
   * The reservation is derived from the staged cluster now (`regions.ts`), so
   * this is checkable at every seat count and geometry rather than asserted for
   * the one case the constant happened to describe.
   */
  it('never draws a card under any seat’s medallion group', () => {
    for (const [label, viewport] of VIEWPORTS) {
      for (const opponents of SEAT_COUNTS) {
        const plane = stage(
          seatTable({
            opponents,
            active: 'p2',
            perms: [
              ...menagerie('p1', 9),
              ...menagerie('p2', 9),
              ...bears('p3', 12),
              ...bears('p4', 6),
            ],
          }),
          viewport,
        );
        for (const region of regionsOf(plane)) {
          for (const other of regionsOf(plane)) {
            for (const render of other.renders) {
              expect(
                rectsOverlap(render.rect, region.cluster.core),
                `${other.seat}'s card under ${region.seat}'s medallion at ${label}, ${opponents} opponents`,
              ).toBe(false);
            }
          }
        }
      }
    }
  });

  it('never overlaps one seat region with another', () => {
    for (const [, viewport] of VIEWPORTS) {
      for (const opponents of SEAT_COUNTS) {
        const regions = regionsOf(stage(seatTable({ opponents, active: 'p2' }), viewport));
        for (let i = 0; i < regions.length; i += 1) {
          for (let j = i + 1; j < regions.length; j += 1) {
            expect(rectsOverlap(regions[i]!.rect, regions[j]!.rect)).toBe(false);
          }
        }
      }
    }
  });
});

describe('battlefield composition — rows read outward from their owner', () => {
  const board = (seat: string) => [
    ...menagerie(seat, 2),
    { id: `${seat}_art`, controller: seat, type_line: 'Artifact', name: 'Urn' },
    ...basics(seat, 2, 'Forest'),
  ];

  it('puts the receiver’s creatures nearest the arena and its lands nearest itself', () => {
    const plane = stage(seatTable({ opponents: 3, active: 'p2', perms: board('p1') }));
    const rows = plane.receiver!.renders;
    const creatureY = Math.min(...rows.filter((r) => r.row === 'creatures').map((r) => r.rect.y));
    const landY = Math.min(...rows.filter((r) => r.row === 'lands').map((r) => r.rect.y));
    // The receiver's own edge is the bottom of the plane, so its lands sit below.
    expect(creatureY).toBeLessThan(landY);
  });

  it('reverses the focused opponent’s rows, because its own edge is the top', () => {
    const plane = stage(seatTable({ opponents: 3, active: 'p2', perms: board('p2') }));
    const rows = plane.farSide!.renders;
    const creatureY = Math.min(...rows.filter((r) => r.row === 'creatures').map((r) => r.rect.y));
    const landY = Math.min(...rows.filter((r) => r.row === 'lands').map((r) => r.rect.y));
    // Lands hug the top edge; creatures face the corridor. This is the baseline's
    // arrangement and the inverse of the receiver's.
    expect(landY).toBeLessThan(creatureY);
  });

  it('keeps a flank wing reading the same way the receiver does', () => {
    const plane = stage(seatTable({ opponents: 3, active: 'p2', perms: board('p3') }), {
      width: 1680,
      height: 945,
    });
    const rows = plane.wings.find((w) => w.seat === 'p3')!.renders;
    const creatureY = Math.min(...rows.filter((r) => r.row === 'creatures').map((r) => r.rect.y));
    const landY = Math.min(...rows.filter((r) => r.row === 'lands').map((r) => r.rect.y));
    expect(creatureY).toBeLessThan(landY);
  });
});

describe('battlefield composition — the reserved cell is the card’s real box', () => {
  it('reserves the land tile’s box, not the square plaque’s, at every tier', () => {
    // `cellSize` read `TIER[tier].w/.h` and so reserved a square for a land at
    // every tier. A land is a 1.45 resource tile (card-representation §3.1/§4),
    // and the reserved cell must be exactly what the face draws.
    // The four plane tiers, typed as the plane's own `RenderTier` and pinned to
    // the card layer's list so the two can never drift.
    const tiers = ['chip', 'mini', 'support', 'field'] as const;
    expect([...BATTLEFIELD_TIERS]).toEqual([...tiers]);
    for (const tier of tiers) {
      expect(cellSize(tier, false, 'land')).toEqual(CARD_BOX[tier].land);
      expect(cellSize(tier, false, 'permanent')).toEqual(CARD_BOX[tier].permanent);
      expect(CARD_BOX[tier].land.h).toBeLessThanOrEqual(CARD_BOX[tier].permanent.h);
      // Tapped reserves the swept bounding box, from the same source.
      expect(cellSize(tier, true, 'land')).toEqual(faceFootprint(tier, true, 'land'));
    }
    // `chip` is the one tier where the tile and the plaque share a box (the
    // chip's authored `landH` equals its `h`); everywhere else the tile is
    // strictly shorter, tapped or not.
    for (const tier of ['mini', 'support', 'field'] as const) {
      expect(CARD_BOX[tier].land.h).toBeLessThan(CARD_BOX[tier].permanent.h);
      expect(cellSize(tier, true, 'land')).not.toEqual(cellSize(tier, true, 'permanent'));
    }
  });

  it('stages a land row at the tile box and a creature row at the plaque box', () => {
    const perms = [...menagerie('p2', 2), ...basics('p2', 3, 'Forest')];
    const plane = stage(seatTable({ opponents: 3, active: 'p2', perms }));
    for (const render of plane.farSide!.renders) {
      const kind = render.row === 'lands' ? 'land' : 'permanent';
      expect({ row: render.row, box: { w: render.rect.w, h: render.rect.h } }).toEqual({
        row: render.row,
        box: faceFootprint(render.tier, render.tapped, kind),
      });
    }
  });

  it('marks a lands-row permanent as a tile in the display data the face reads', () => {
    // The silhouette is display glue over the server type line, derived in three
    // places — the fold key, the live face renderer, and this mapping. They must
    // agree, or a card's reserved cell and its drawn box disagree.
    const view = seatTable({
      opponents: 3,
      active: 'p2',
      perms: [...menagerie('p2', 1), ...basics('p2', 1, 'Forest')],
    });
    const plane = stage(view);
    for (const render of plane.farSide!.renders) {
      expect(planeDisplayData(view, undefined, render).landTile).toBe(render.row === 'lands');
    }
  });

  it('separates the two silhouettes in the ×N fold key', () => {
    // The plane's fold key runs `cardVisualSignature` over the same display data
    // the face draws. With the tile flag carried, two otherwise identical cards
    // that draw different silhouettes can never collapse into one pile — the
    // property `regions.ts` now sets `landTile` for.
    const base = { name: 'Twinned', typeLine: 'Land', colorIdentity: 'C' } as const;
    expect(cardVisualSignature({ ...base, landTile: true })).not.toBe(
      cardVisualSignature({ ...base, landTile: false }),
    );
  });
});

describe('battlefield composition — the ×N pile reserves what it actually sweeps', () => {
  /** A far-side board crowded enough to reach the folding rung. */
  function foldedFar(perms: Parameters<typeof seatTable>[0]['perms']) {
    return stage(seatTable({ opponents: 3, active: 'p2', perms }));
  }

  it('leaves headroom above a fold for its top-edge ×N tab', () => {
    // The count moved from a corner badge to a top-edge tab overhanging the card
    // by half its own height (#529 §7.4), so a folded line must start lower
    // inside its content area than an unfolded one at the same tier — otherwise
    // the tab collides with whatever is above it.
    const folded = foldedFar(bears('p2', 24));
    const single = foldedFar(bears('p2', 24, { prefix: 'one' }).slice(0, 1));
    const pile = folded.farSide!.renders.find((r) => r.stackCount > 1)!;
    const lone = single.farSide!.renders[0]!;
    expect(pile.stackCount).toBe(24);
    expect(lone.stackCount).toBe(1);
    // Both rows are vertically centred in the same slot; the folded one carries
    // the tab reservation, so its top edge sits lower than the tab clearance
    // would allow it to if nothing were reserved.
    expect(tabClearance(pile.tier)).toBeGreaterThan(0);
    expect(pile.rect.y - folded.farSide!.rect.y).toBeGreaterThanOrEqual(tabClearance(pile.tier));
  });

  it('clears the down-and-left splay between a fold and its left neighbour', () => {
    // The splay steps down-and-LEFT (#529 §15.3), so a pile's depth reaches back
    // over the card before it. The gap ahead of a folded cell has to absorb it.
    const perms = [
      ...menagerie('p2', 1),
      ...bears('p2', 24, { prefix: 'fold' }),
      { id: 'tail', controller: 'p2', name: 'Tail', power: '7' },
    ];
    const plane = foldedFar(perms);
    const row = plane
      .farSide!.renders.filter((r) => r.row === 'creatures')
      .sort((a, b) => a.rect.x - b.rect.x);
    const foldIndex = row.findIndex((r) => r.stackCount > 1);
    expect(foldIndex).toBeGreaterThan(0);
    const fold = row[foldIndex]!;
    const before = row[foldIndex - 1]!;
    const gap = fold.rect.x - (before.rect.x + before.rect.w);
    // Wider than the plain card gap by the swept overhang.
    expect(gap).toBeGreaterThan(PLANE.cardGap);
    expect(gap).toBeGreaterThanOrEqual(PLANE.cardGap + splayClearance(fold.tier).left);
  });
});

describe('battlefield composition — the degradation ladder is unchanged in shape', () => {
  it('walks the ladder in order as one seat’s board grows, others untouched', () => {
    const rungs = [4, 8, 30, 60].map((n) => {
      const plane = stage(
        seatTable({
          opponents: 3,
          active: 'p2',
          perms: [...menagerie('p2', n), ...menagerie('p1', 2)],
        }),
      );
      return { far: plane.farSide!.rung, receiver: plane.receiver!.rung };
    });
    // Monotone non-decreasing on the crowded region…
    for (let i = 1; i < rungs.length; i += 1) {
      expect(rungs[i]!.far).toBeGreaterThanOrEqual(rungs[i - 1]!.far);
    }
    expect(rungs[0]!.far).toBe(0);
    expect(rungs[rungs.length - 1]!.far).toBeGreaterThanOrEqual(3);
    // …and flat on the sparse one: the ladder engages per region, independently.
    expect(rungs.every((r) => r.receiver === 0)).toBe(true);
  });

  it('never digests the receiver or the focused board, at any count or geometry', () => {
    for (const [, viewport] of VIEWPORTS) {
      for (const opponents of SEAT_COUNTS) {
        const plane = stage(
          seatTable({
            opponents,
            active: 'p2',
            perms: [...menagerie('p1', 60), ...menagerie('p2', 60)],
          }),
          viewport,
        );
        for (const region of [plane.receiver!, plane.farSide!]) {
          expect(region.rung).toBeLessThanOrEqual(3);
          expect(region.digest).toBeUndefined();
          expect(region.renders).toHaveLength(60);
        }
      }
    }
  });

  it('keeps every render hotspot at the 44 px floor under the 240-permanent stress', () => {
    const perms = [
      ...menagerie('p1', 60),
      ...menagerie('p2', 60),
      ...bears('p3', 60),
      ...bears('p4', 60, { prefix: 'p4' }),
    ];
    const plane = stage(seatTable({ opponents: 3, active: 'p2', perms }), {
      width: 1680,
      height: 945,
    });
    for (const region of regionsOf(plane)) {
      for (const render of region.renders) {
        expect(render.hitRect.w).toBeGreaterThanOrEqual(PLANE.minHit);
        expect(render.hitRect.h).toBeGreaterThanOrEqual(PLANE.minHit);
      }
    }
  });
});

describe('battlefield composition — the scene node budget', () => {
  /** Reconcile a staged plane into a detached root and measure what it costs. */
  function measure(
    seats: number,
    perSeat: number,
  ): { total: number; faces: number; maxPerFace: number } {
    const ids = Array.from({ length: seats }, (_, i) => `p${i + 1}`);
    // Pairwise-distinct permanents, so ×N folding never masks the real cost.
    const perms = ids.flatMap((id) =>
      menagerie(id, perSeat).map((perm, i) => ({
        ...perm,
        name: `${id} Beast ${i}`,
        power: String(1 + (i % 9)),
      })),
    );
    const view = seatTable({ opponents: seats - 1, active: 'p2', perms });
    const plane = stage(view, { width: 1680, height: 945 });
    const root = document.createElement('div');
    const reconciler = new PlaneReconciler(root, {
      face: cardFaceRenderer((render) => planeDisplayData(view, undefined, render)),
    });
    reconciler.rebuild(plane);
    let maxPerFace = 0;
    const faces = root.querySelectorAll('[data-entity-id]');
    faces.forEach((face) => {
      maxPerFace = Math.max(maxPerFace, face.querySelectorAll('*').length + 1);
    });
    return { total: root.querySelectorAll('*').length, faces: faces.length, maxPerFace };
  }

  it('stays far inside the ≤ 15 000-node scene ceiling at 4 and 6 seats', () => {
    // presentation-budgets §Performance. Measured on the reconciled subtree —
    // the DOM that scales with the board — not estimated from the anatomy.
    for (const [seats, perSeat] of [
      [4, 30],
      [4, 60],
      [6, 20],
      [6, 40],
    ] as const) {
      const { total } = measure(seats, perSeat);
      expect({ seats, perSeat, total, within: total <= SCENE_DOM_CEILING }).toEqual({
        seats,
        perSeat,
        total,
        within: true,
      });
      // Headroom is the point: the ceiling should not be a near miss.
      expect(total).toBeLessThan(SCENE_DOM_CEILING / 4);
    }
  });

  it('keeps every battlefield card face at or under 12 nodes', () => {
    // The per-face half of the same budget. A face that grows past this is a
    // card-layer regression, and it shows up here before the scene ceiling does.
    for (const [seats, perSeat] of [
      [4, 60],
      [6, 40],
    ] as const) {
      const { maxPerFace, faces } = measure(seats, perSeat);
      expect(faces).toBeGreaterThan(0);
      expect({ seats, maxPerFace }).toEqual({ seats, maxPerFace });
      expect(maxPerFace).toBeLessThanOrEqual(12);
    }
  });
});

describe('battlefield composition — one GameView rebuilds it', () => {
  it('is a pure function of (view, viewport, staging)', () => {
    const view = seatTable({
      opponents: 3,
      active: 'p2',
      perms: [...menagerie('p2', 6), ...basics('p1', 4, 'Island')],
    });
    const viewport = { width: 1680, height: 945 };
    expect(stage(view, viewport)).toEqual(stage(view, viewport));
    // A bystander mounting mid-game derives the identical composition.
    expect(stage(view, viewport, { focusSeat: 'p3' })).toEqual(
      stage(view, viewport, { focusSeat: 'p3' }),
    );
  });

  it('re-stages a wing into the focused position without losing its rack', () => {
    const view = seatTable({ opponents: 3, active: 'p2', perms: menagerie('p3', 4) });
    const viewport = { width: 1680, height: 945 };
    const before = stage(view, viewport);
    const after = stage(view, viewport, { focusSeat: 'p3' });
    expect(before.wings.map((w) => w.seat)).toContain('p3');
    expect(after.farSide!.seat).toBe('p3');
    // The promoted seat keeps four zone anchors either side of the swap, and
    // every one of them resolves to a real rect at 0 ms.
    const zonesOf = (plane: StagedPlane, seat: string) =>
      regionsOf(plane)
        .find((r) => r.seat === seat)!
        .rack.slots.map((s) => s.zone);
    expect(zonesOf(after, 'p3')).toEqual(zonesOf(before, 'p3'));
    // …and the demoted seat keeps its own.
    expect(zonesOf(after, 'p2')).toEqual(zonesOf(before, 'p2'));
  });

  it('keeps an eliminated seat staged, with its public zones still anchored', () => {
    const plane = stage(seatTable({ opponents: 3, active: 'p2', eliminated: ['p4'] }));
    const wing = plane.wings.find((w) => w.seat === 'p4')!;
    expect(wing.eliminated).toBe(true);
    expect(wing.rack.slots.map((s) => s.zone)).toContain('graveyard');
    expect(wing.rack.slots.find((s) => s.zone === 'library')!.count).toBe(60);
    expect(inPropPocket(toFractionRect(wing.crest, DESKTOP))).toBe(false);
  });
});
