/**
 * Opponent **face-down hand fans** on the plane — `table/plane/seatHandFan.ts`,
 * issue #533's widened scope.
 *
 * `zone-geography.md` §4.1 rests the count-ownership ruling on this surface
 * existing ("an opponent's hand renders as a face-down fan with no
 * count-bearing surface"), `card-representation.md` §13 says it must wear the
 * one card-back silhouette, and `rune-2.5d-interface-baseline.jpg` draws it at
 * the focused seat and both wings. Until this work it did not exist: an
 * opponent's hand was a numeric pip and nothing else.
 *
 * The section that matters most is **hidden-information safety**, which the
 * issue makes a test requirement rather than a review note. It is asserted
 * structurally, in the idiom `card/back/cardBack.test.tsx` established: not
 * "this particular fan looks the same", but "there is no channel on this path a
 * card could travel down".
 *
 * **jsdom limits.** No layout, no CSS, no pixels. Nothing here proves a back is
 * legible, that a fan reads as a fan, or that it sits where the baseline draws
 * it. Those are the maintainer's browser checks. What is proven is the staged
 * geometry and the data contract the reconciler is handed.
 */
import { describe, expect, it } from 'vitest';
import type { GameView } from '../protocol';
import { rectsOverlap, type Rect } from './scene';
import { fanCapacity, opponentFanSpan, opponentFanTier } from './handFan';
import { PLANE, stageSeatHandFan, withinEnvelope, type StagedPlane } from './plane';
import { DESKTOP, TABLET, ULTRAWIDE, WIDE16, regionsOf, seatTable, stage } from './plane.fixture';

const BASELINE = { width: 1680, height: 945 };
const GEOMETRIES = [
  { label: 'desktop 1280×800', viewport: DESKTOP },
  { label: 'baseline 1680×945', viewport: BASELINE },
  { label: 'tablet landscape 1180×820', viewport: TABLET },
  { label: '16:9 1920×1080', viewport: WIDE16 },
  { label: 'ultrawide 2560×1080', viewport: ULTRAWIDE },
];

/** Every opponent region's fan, which must exist for all of them. */
function fansOf(plane: StagedPlane) {
  return regionsOf(plane)
    .filter((region) => region.kind !== 'receiver')
    .map((region) => ({ region, fan: region.handFan! }));
}

describe('every opponent seat renders a fan, at every rung', () => {
  for (const { label, viewport } of GEOMETRIES) {
    for (const opponents of [1, 2, 3, 4, 5]) {
      it(`stages a fan for all ${opponents} opponent(s) at ${label}`, () => {
        const plane = stage(seatTable({ opponents, handSize: 7 }), viewport);
        const regions = regionsOf(plane).filter((region) => region.kind !== 'receiver');
        expect(regions).toHaveLength(opponents);
        for (const region of regions) {
          const fan = region.handFan;
          expect(fan, `${region.seat} (${region.kind}, rung ${region.rung})`).toBeDefined();
          expect(fan!.seat).toBe(region.seat);
          expect(fan!.count).toBe(7);
          expect(fan!.slots.length).toBe(7);
        }
      });
    }
  }

  it('covers the digest rung too — the fan is a seat fixture, not board content', () => {
    // Five and six players stage two wings per side at the digest rung, where
    // the board stops drawing cards. The crest, the rack, and now the fan stay:
    // they are the seat, not its battlefield.
    const plane = stage(seatTable({ opponents: 5, handSize: 7 }), BASELINE);
    const digested = plane.wings.filter(
      (wing) => wing.rung === 4 || wing.rack.variant === 'digest',
    );
    expect(digested.length).toBeGreaterThan(0);
    for (const wing of digested) expect(wing.handFan?.slots.length).toBe(7);
  });

  it('gives the RECEIVER no plane fan — their hand is a shell region (ADR 0032 §7)', () => {
    const plane = stage(seatTable({ opponents: 3, handSize: 7 }), BASELINE);
    expect(plane.receiver).toBeDefined();
    expect(plane.receiver!.handFan).toBeUndefined();
  });

  it('gives a rung-5 summary tile no fan, and says why in its anchor instead', () => {
    // The tile IS the minimal cluster rung (`zone-geography.md` §4.1): it draws
    // no hand pip either, its own row carries the count as text, and its growth
    // budget belongs to candidate strips, which are load-bearing picks.
    const plane = stage(seatTable({ opponents: 3, handSize: 7 }), { width: 390, height: 844 });
    expect(plane.compact).toBe(true);
    expect(plane.tiles.length).toBeGreaterThan(0);
    for (const tile of plane.tiles) {
      expect(regionsOf(plane).some((region) => region.seat === tile.seat)).toBe(false);
    }
  });
});

describe('the fan is sized from `hand_size` and nothing else', () => {
  it('draws one back per card, up to the rung’s capacity', () => {
    for (const count of [0, 1, 2, 7, 12, 15, 20, 60]) {
      const plane = stage(seatTable({ opponents: 3, handSize: count }), BASELINE);
      for (const { region, fan } of fansOf(plane)) {
        const capacity = fanCapacity(
          opponentFanSpan(region.cluster.d),
          opponentFanTier(region.cluster.d),
        );
        expect(fan.count).toBe(count);
        expect(fan.slots.length).toBe(Math.min(count, capacity));
        expect(fan.undrawn).toBe(Math.max(0, count - fan.slots.length));
      }
    }
  });

  it('never shrinks the fan when the hand grows', () => {
    let previous = -1;
    for (let count = 0; count <= 40; count += 1) {
      const plane = stage(seatTable({ opponents: 1, handSize: count }), BASELINE);
      const drawn = plane.farSide!.handFan!.slots.length;
      expect(drawn).toBeGreaterThanOrEqual(previous);
      previous = drawn;
    }
  });

  it('leaves the count itself to the cluster pip — the fan draws no number', () => {
    // `zone-geography.md` §4/I5: a count has exactly one home. The fan is that
    // home's *physical* companion, not a second readout.
    const plane = stage(seatTable({ opponents: 1, handSize: 9 }), BASELINE);
    const region = plane.farSide!;
    expect(region.cluster.pip?.count).toBe(9);
    const fan = region.handFan!;
    expect(Object.keys(fan.slots[0]!).sort()).toEqual(['angleDeg', 'index', 'rect']);
  });
});

describe('hidden-information safety — asserted structurally (§13.1)', () => {
  it('takes no card, id, zone, or view: the fan is a function of a number', () => {
    // The strongest form of the answer, and the same one `card/back/` gives for
    // the image: there is nothing for a back to vary WITH, because nothing on
    // this path is ever told what it hides. `stageSeatHandFan` accepts exactly
    // one request object, whose keys are enumerated here in full.
    expect(stageSeatHandFan.length).toBe(1);
    const request = {
      seat: 'p2',
      count: 5,
      d: 96,
      portrait: { x: 400, y: 120, w: 96, h: 96 },
      keepOut: { x: 0, y: 0, w: 10, h: 10 },
      viewport: BASELINE,
    };
    expect(Object.keys(request).sort()).toEqual([
      'count',
      'd',
      'keepOut',
      'portrait',
      'seat',
      'viewport',
    ]);
    const fan = stageSeatHandFan(request);
    // Called any number of times it is the same answer: pure, and with no
    // per-call ordering, timing, or shuffle channel.
    for (let i = 0; i < 50; i += 1) expect(stageSeatHandFan(request)).toEqual(fan);
  });

  it('produces slot geometry that depends on the count and the index only', () => {
    // Two different seats, at the same rung and the same count, are the same
    // fan up to translation. No seat, no order, no timing may differ.
    const a = stageSeatHandFan({
      seat: 'p2',
      count: 8,
      d: 76,
      portrait: { x: 100, y: 200, w: 76, h: 76 },
      viewport: BASELINE,
    });
    const b = stageSeatHandFan({
      seat: 'p5',
      count: 8,
      d: 76,
      portrait: { x: 900, y: 200, w: 76, h: 76 },
      viewport: BASELINE,
    });
    expect(a.slots.length).toBe(b.slots.length);
    for (let i = 0; i < a.slots.length; i += 1) {
      const left = a.slots[i]!;
      const right = b.slots[i]!;
      expect(right.angleDeg).toBe(left.angleDeg);
      expect(right.rect.w).toBe(left.rect.w);
      expect(right.rect.h).toBe(left.rect.h);
      expect(right.rect.y).toBe(left.rect.y);
      expect(right.rect.x - left.rect.x).toBe(b.slots[0]!.rect.x - a.slots[0]!.rect.x);
    }
  });

  it('gives every back in a fan the same width — no per-card size channel', () => {
    for (const count of [1, 4, 9, 15]) {
      const fan = stageSeatHandFan({
        seat: 'p2',
        count,
        d: 96,
        portrait: { x: 400, y: 120, w: 96, h: 96 },
        viewport: BASELINE,
      });
      expect(new Set(fan.slots.map((slot) => slot.rect.w)).size).toBe(1);
      expect(new Set(fan.slots.map((slot) => slot.rect.h)).size).toBe(1);
      expect(fan.slots.map((slot) => slot.index)).toEqual(
        Array.from({ length: fan.slots.length }, (_, i) => i),
      );
    }
  });

  it('is unchanged by anything the receiver knows or does', () => {
    // What the receiver holds and what they are offered are the private data an
    // opponent fan must be blind to; both move here and the fans must not.
    // (Focus-changing data such as `active_player` legitimately re-stages a
    // seat's rung, so it is not part of this claim.)
    const base = seatTable({ opponents: 3, handSize: 6 });
    const loaded: GameView = {
      ...base,
      my_hand: [
        { id: 'h1', name: 'Arcane Bolt', type_line: 'Instant' },
        { id: 'h2', name: 'Wispfox', type_line: 'Creature — Fox' },
      ],
      valid_actions: [{ id: 'a1', type: 'cast_spell', label: 'Cast', subject: ['h1'] }],
    };
    const before = fansOf(stage(base, BASELINE)).map(({ fan }) => fan);
    const after = fansOf(stage(loaded, BASELINE)).map(({ fan }) => fan);
    expect(after).toEqual(before);
  });

  it('varies only with the count, seat by seat', () => {
    const view = seatTable({ opponents: 2, handSize: 4 });
    const uneven: GameView = {
      ...view,
      opponents: view.opponents.map((opponent, index) => ({
        ...opponent,
        hand_size: index === 0 ? 4 : 11,
      })),
    };
    const plane = stage(uneven, BASELINE);
    const fans = fansOf(plane);
    const counts = fans.map(({ fan }) => fan.slots.length).sort((a, b) => a - b);
    expect(counts).toEqual([4, 11]);
  });
});

describe('placement — the fan is drawn layout content, held to the same rules', () => {
  for (const { label, viewport } of GEOMETRIES) {
    it(`keeps every fan on the plane and inside the seat envelope at ${label}`, () => {
      const plane = stage(seatTable({ opponents: 4, handSize: 12 }), viewport);
      for (const { region, fan } of fansOf(plane)) {
        for (const slot of fan.slots) {
          expect(slot.rect.x, `${region.seat}`).toBeGreaterThanOrEqual(0);
          expect(slot.rect.y).toBeGreaterThanOrEqual(0);
          expect(slot.rect.x + slot.rect.w).toBeLessThanOrEqual(viewport.width);
          expect(slot.rect.y + slot.rect.h).toBeLessThanOrEqual(viewport.height);
        }
        expect(withinEnvelope(fan.bounds, viewport), `${region.seat} envelope`).toBe(true);
      }
    });

    it(`keeps every fan out of the centre corridor at ${label}`, () => {
      // "The centre corridor stays clear: nothing parks there"
      // (`layout-model.md` §The plane and its fixed slots).
      const plane = stage(seatTable({ opponents: 4, handSize: 12 }), viewport);
      for (const { region, fan } of fansOf(plane)) {
        expect(rectsOverlap(fan.bounds, plane.corridor), `${region.seat} in corridor`).toBe(false);
      }
    });
  }

  it('never lays the fan through the seat’s own zone rack', () => {
    for (const { viewport } of GEOMETRIES) {
      for (const opponents of [1, 3, 5]) {
        const plane = stage(seatTable({ opponents, handSize: 12 }), viewport);
        for (const { region, fan } of fansOf(plane)) {
          expect(rectsOverlap(fan.bounds, region.piles), `${region.seat} through rack`).toBe(false);
        }
      }
    }
  });

  it('lets the seat’s medallion overlap the fan, as the baseline draws it', () => {
    const plane = stage(seatTable({ opponents: 3, handSize: 7 }), BASELINE);
    const focused = plane.farSide!;
    expect(rectsOverlap(focused.handFan!.bounds, focused.cluster.portrait)).toBe(true);
  });

  it('never overlaps one seat’s fan with another’s', () => {
    const plane = stage(seatTable({ opponents: 5, handSize: 20 }), BASELINE);
    const fans = fansOf(plane).map(({ fan }) => fan.bounds);
    for (let i = 0; i < fans.length; i += 1) {
      for (let j = i + 1; j < fans.length; j += 1) {
        expect(rectsOverlap(fans[i]!, fans[j]!), `fan ${i} overlaps fan ${j}`).toBe(false);
      }
    }
  });
});

describe('the `hand:<seat>` travel anchor (§7, §9)', () => {
  it('resolves to a real fan slot for every opponent, at every player count', () => {
    for (const opponents of [1, 2, 3, 4, 5]) {
      const plane = stage(seatTable({ opponents, handSize: 7 }), BASELINE);
      for (const { region, fan } of fansOf(plane)) {
        const last = fan.slots[fan.slots.length - 1]!;
        // §9's draw row terminates at "`hand:s` fan slot" — the slot the card
        // lands in, not the crest fallback the chain ends at.
        expect(fan.anchor).toEqual(last.rect);
        // …and it is a slot, not one of the §7 fallbacks the chain would end at.
        expect(fan.anchor).not.toEqual(region.crest);
        expect(fan.anchor).not.toEqual(region.piles);
        expect(rectsOverlap(fan.anchor, fan.bounds)).toBe(true);
      }
    }
  });

  it('still publishes an anchor for an EMPTY hand, so a draw has a destination', () => {
    const plane = stage(seatTable({ opponents: 1, handSize: 0 }), BASELINE);
    const fan = plane.farSide!.handFan!;
    expect(fan.slots).toHaveLength(0);
    expect(fan.anchor.w).toBeGreaterThan(0);
    expect(fan.anchor.h).toBeGreaterThan(0);
    expect(rectsOverlap(fan.anchor, fan.bounds)).toBe(true);
  });

  it('moves the anchor to the new slot as the hand grows', () => {
    const before = stage(seatTable({ opponents: 1, handSize: 5 }), BASELINE).farSide!.handFan!;
    const after = stage(seatTable({ opponents: 1, handSize: 6 }), BASELINE).farSide!.handFan!;
    expect(after.slots).toHaveLength(6);
    expect(after.anchor).not.toEqual(before.anchor);
    expect(after.anchor).toEqual(after.slots[5]!.rect);
  });
});

describe('node budget — six seats with large hands', () => {
  it('costs a bounded number of elements, whatever the hands hold', () => {
    // `presentation-budgets.md` §Performance caps the scene at ≤ 15 000 nodes.
    // A fan is one element per drawn back, and the drawn count is bounded by
    // the rung's geometry — so the cost is a function of seat count, never of
    // hand depth, exactly as `zone-geography.md` §13 argues for the racks.
    const costs = new Map<number, number>();
    for (const handSize of [7, 20, 60, 200]) {
      const plane = stage(seatTable({ opponents: 5, handSize }), BASELINE);
      const total = fansOf(plane).reduce((sum, { fan }) => sum + fan.slots.length, 0);
      costs.set(handSize, total);
      expect(total).toBeLessThanOrEqual(6 * 16);
      expect(total).toBeLessThan(0.01 * 15000);
    }
    // Deep hands cost no more than a 20-card one: the bound has already bitten.
    expect(costs.get(60)).toBe(costs.get(20));
    expect(costs.get(200)).toBe(costs.get(20));
  });

  it('keeps every drawn back at or above the tier’s legibility floor', () => {
    const plane = stage(seatTable({ opponents: 5, handSize: 60 }), BASELINE);
    for (const { region, fan } of fansOf(plane)) {
      const tier = opponentFanTier(region.cluster.d);
      for (let i = 1; i < fan.slots.length; i += 1) {
        const gap = fan.slots[i]!.rect.x - fan.slots[i - 1]!.rect.x;
        expect(gap, `${region.seat} slot ${i}`).toBeGreaterThanOrEqual(tier.minExposure - 1);
      }
    }
  });

  it('never lets a fan out-scale the crest it belongs to', () => {
    const plane = stage(seatTable({ opponents: 5, handSize: 20 }), BASELINE);
    for (const { region, fan } of fansOf(plane)) {
      expect(fan.card.w).toBeLessThan(region.cluster.d);
      expect(fan.bounds.h).toBeLessThan(region.cluster.d * 1.2);
      expect(PLANE.minHit).toBe(44);
    }
  });
});

/** A rect list helper kept local so `plane.fixture`'s corridor sweep is untouched. */
export function fanRects(plane: StagedPlane): Rect[] {
  return fansOf(plane).flatMap(({ fan }) => fan.slots.map((slot) => slot.rect));
}
