/**
 * The shared hand-fan curve family (issue #533) — `table/handFan.ts`.
 *
 * Two claims are load-bearing and are tested as such:
 *
 * 1. **There is one curve, not two.** The receiver's hand and every opponent's
 *    face-down fan differ by a {@link FanTier} record and nothing else, so the
 *    normalised shape of a 7-card fan is identical at both tiers. If someone
 *    later writes a second fan, the family test is what fails.
 * 2. **The 44 px floor is arithmetic.** `layout-model.md` §Stress dispositions
 *    routes large hands to "fan compresses spacing and rotation before card
 *    size; when exposed spacing would drop below the 44 px floor, the fan pages
 *    (page size derived from the floor)". The page size *is* derived from the
 *    floor here, so the exhaustive sweep below cannot be satisfied by tuning —
 *    it either holds for every count at every supported band width or the
 *    derivation is wrong.
 *
 * **What jsdom cannot show.** No layout, no CSS, no pixels. Nothing here proves
 * that the fan looks like the baseline, that a 44 px strip is actually
 * clickable through the overlap, or that a rotated card's title bar clears its
 * neighbour. Those are the maintainer's browser checks. What is proven is the
 * geometry the stylesheet is handed.
 */
import { describe, expect, it } from 'vitest';
import { TIER } from '../tokens';
import {
  FAN,
  LOCAL_FAN_TIER,
  fanAngle,
  fanCapacity,
  fanDip,
  fanFraction,
  fanInset,
  fanPageOf,
  fanPageRange,
  fanPlan,
  fanUsableSpan,
  handCountBand,
  localFanPlan,
  opponentFanSpan,
  opponentFanTier,
} from './handFan';
import { SHELL, handFanSpacing, shellBands } from './live/shellLayout';

/** The supported viewports, as `shellLayout.test.ts` enumerates them. */
const SUPPORTED = [
  { label: 'desktop 1280×720', width: 1280, height: 720 },
  { label: 'desktop 1280×800', width: 1280, height: 800 },
  { label: 'desktop 1440×900', width: 1440, height: 900 },
  { label: 'desktop 1680×945', width: 1680, height: 945 },
  { label: 'desktop 2048×1024', width: 2048, height: 1024 },
  { label: 'tablet landscape 1180×820', width: 1180, height: 820 },
  { label: 'phone portrait 390×844', width: 390, height: 844 },
];

/** Every band width the receiver's fan is ever laid out in. */
const BAND_WIDTHS = SUPPORTED.map((vp) => shellBands(vp).hand.w);

describe('count bands — 0–7, 8–12, 13–20, 20+', () => {
  it('assigns the four bands the issue names, at their exact boundaries', () => {
    expect(handCountBand(0)).toBe('opening');
    expect(handCountBand(7)).toBe('opening');
    expect(handCountBand(8)).toBe('wide');
    expect(handCountBand(12)).toBe('wide');
    expect(handCountBand(13)).toBe('deep');
    expect(handCountBand(20)).toBe('deep');
    expect(handCountBand(21)).toBe('overflow');
    expect(handCountBand(60)).toBe('overflow');
  });

  it('closes the overlap band by band, never below the tier floor', () => {
    const targets = [FAN.target.opening, FAN.target.wide, FAN.target.deep];
    for (let i = 1; i < targets.length; i += 1) {
      expect(targets[i]!).toBeLessThan(targets[i - 1]!);
    }
    // The deepest drawn band still clears the 44 px floor on its own terms, so
    // the floor only ever binds through the *span*, never through the band.
    expect(FAN.target.deep * TIER.hand.w).toBeGreaterThanOrEqual(SHELL.minHit);
    expect(FAN.target.overflow).toBe(0);
  });
});

describe('the curve — one family, two tiers', () => {
  it('places the endpoints at 0 and 1 and centres a single card', () => {
    expect(fanFraction(0, 7)).toBe(0);
    expect(fanFraction(6, 7)).toBe(1);
    expect(fanFraction(0, 1)).toBe(0.5);
  });

  it('rotates 0 at the centre and reaches the cap at both ends, symmetrically', () => {
    const n = 7;
    const angles = Array.from({ length: n }, (_, i) => fanAngle(i, n, LOCAL_FAN_TIER));
    expect(angles[3]).toBeCloseTo(0, 6);
    expect(angles[0]).toBeCloseTo(-angles[6]!, 6);
    for (let i = 1; i < n; i += 1) expect(angles[i]!).toBeGreaterThan(angles[i - 1]!);
    // Outer cards angle more than centre cards, at every count.
    for (const count of [2, 3, 5, 7, 12, 20, 40]) {
      for (let i = 0; i < count; i += 1) {
        const a = Math.abs(fanAngle(i, count, LOCAL_FAN_TIER));
        expect(a).toBeLessThanOrEqual(LOCAL_FAN_TIER.maxDeg + 1e-9);
        expect(a).toBeGreaterThanOrEqual(
          Math.abs(fanAngle(count >> 1, count, LOCAL_FAN_TIER)) - 1e-9,
        );
      }
    }
  });

  it('grows the rotation with the hand until it saturates at the cap', () => {
    const outer = (n: number): number => Math.abs(fanAngle(0, n, LOCAL_FAN_TIER));
    expect(outer(2)).toBeCloseTo(FAN.stepDeg / 2, 6);
    expect(outer(3)).toBeCloseTo(FAN.stepDeg, 6);
    expect(outer(12)).toBe(LOCAL_FAN_TIER.maxDeg);
    expect(outer(20)).toBe(LOCAL_FAN_TIER.maxDeg);
  });

  it('dips parabolically — flat at the centre, deepest at both ends', () => {
    const n = 9;
    const dips = Array.from({ length: n }, (_, i) => fanDip(i, n, LOCAL_FAN_TIER));
    expect(dips[4]).toBeCloseTo(0, 6);
    expect(dips[0]).toBeCloseTo(dips[8]!, 6);
    expect(dips[0]).toBeCloseTo(LOCAL_FAN_TIER.arcFrac * LOCAL_FAN_TIER.card.h, 6);
    for (let i = 5; i < n; i += 1) expect(dips[i]!).toBeGreaterThan(dips[i - 1]!);
  });

  it('is ONE curve: the local and opponent tiers are the same shape, scaled', () => {
    // This is the whole reason the opponent fan was folded into #533. The two
    // tiers may differ in card box, cap, and floor — but not in shape. Rotation
    // normalised by the tier's own cap, and dip normalised by the tier's own
    // card height, must be identical at every index of every count.
    for (const d of [96, 76, 60, 48]) {
      const opponent = opponentFanTier(d);
      for (const count of [2, 3, 7, 12, 20]) {
        const localCap = Math.min(LOCAL_FAN_TIER.maxDeg, ((count - 1) / 2) * FAN.stepDeg);
        const oppCap = Math.min(opponent.maxDeg, ((count - 1) / 2) * FAN.stepDeg);
        for (let i = 0; i < count; i += 1) {
          expect(fanAngle(i, count, opponent) / oppCap).toBeCloseTo(
            fanAngle(i, count, LOCAL_FAN_TIER) / localCap,
            9,
          );
          expect(fanDip(i, count, opponent) / opponent.card.h).toBeCloseTo(
            fanDip(i, count, LOCAL_FAN_TIER) / LOCAL_FAN_TIER.card.h,
            9,
          );
          expect(fanFraction(i, count)).toBe(fanFraction(i, count));
        }
      }
    }
  });

  it('keeps every opponent tier on the hand card’s silhouette', () => {
    // `card-representation.md` §13: one silhouette across hand, library,
    // travel, and piles. A back is the `hand` tier's aspect at the cluster's
    // scale, so a fan of backs cannot read as a different card family.
    const aspect = TIER.hand.h / TIER.hand.w;
    for (const d of [96, 76, 60, 48]) {
      const tier = opponentFanTier(d);
      expect(tier.card.h / tier.card.w).toBeCloseTo(aspect, 1);
      expect(tier.card.w).toBeGreaterThan(0);
    }
  });
});

describe('compression before paging', () => {
  it('never trades card size away — the box is fixed at every count', () => {
    for (const count of [1, 7, 12, 20, 40]) {
      const plan = fanPlan(count, 882, LOCAL_FAN_TIER);
      expect(plan.span).toBe(882);
      expect(LOCAL_FAN_TIER.card).toEqual({ w: TIER.hand.w, h: TIER.hand.h });
      expect(plan.width).toBeLessThanOrEqual(882);
    }
  });

  it('closes the overlap as the hand grows, before it ever pages', () => {
    // At the widest supported band a hand grows through three bands without
    // paging; the exposure must fall monotonically across them.
    const band = 1250;
    const exposures = [7, 8, 12, 13, 20].map((n) => {
      const plan = fanPlan(n, band, LOCAL_FAN_TIER);
      expect(plan.pages).toBe(1);
      return plan.exposure;
    });
    for (let i = 1; i < exposures.length; i += 1) {
      expect(exposures[i]!).toBeLessThanOrEqual(exposures[i - 1]!);
    }
    expect(exposures[exposures.length - 1]).toBeGreaterThanOrEqual(SHELL.minHit);
  });

  it('keeps a small hand a tight fan instead of stretching it to the band edges', () => {
    // The shipped rule spread every hand across the full band, so a two-card
    // hand sat as two cards pinned to opposite edges with a hole between them.
    const plan = fanPlan(2, 1250, LOCAL_FAN_TIER);
    expect(plan.exposure).toBeCloseTo(FAN.target.opening * TIER.hand.w, 6);
    expect(plan.slack).toBeGreaterThan(0);
    expect(fanInset(plan, LOCAL_FAN_TIER)).toBeGreaterThan(
      LOCAL_FAN_TIER.card.w / 2 + LOCAL_FAN_TIER.gutter,
    );
    // …and the inset only ever GROWS, so #528's containment is strengthened.
    for (const count of [1, 2, 7, 12, 20, 40]) {
      for (const band of BAND_WIDTHS) {
        const p = fanPlan(count, band, LOCAL_FAN_TIER);
        expect(fanInset(p, LOCAL_FAN_TIER)).toBeGreaterThanOrEqual(
          LOCAL_FAN_TIER.card.w / 2 + LOCAL_FAN_TIER.gutter - 1e-9,
        );
      }
    }
  });
});

describe('paging at the 44 px floor', () => {
  it('derives the page size from the floor, so the floor cannot be crossed', () => {
    for (const band of BAND_WIDTHS) {
      for (let count = 1; count <= 60; count += 1) {
        const { plan } = localFanPlan(count, band);
        if (plan.pageSize > 1) {
          expect(plan.exposure, `band=${band} count=${count}`).toBeGreaterThanOrEqual(
            SHELL.minHit - 1e-9,
          );
        }
        expect(plan.pageSize).toBeGreaterThanOrEqual(1);
        expect(plan.pages * plan.pageSize).toBeGreaterThanOrEqual(count);
      }
    }
  });

  it('pages exactly when the whole hand cannot hold the floor', () => {
    for (const band of BAND_WIDTHS) {
      const capacity = fanCapacity(band, LOCAL_FAN_TIER);
      expect(localFanPlan(capacity, band).plan.pages).toBe(1);
      expect(localFanPlan(capacity + 1, band).plan.pages).toBeGreaterThan(1);
      // The gate #528 left for this work agrees with the plan at the boundary.
      expect(handFanSpacing(capacity, band)).toBeGreaterThanOrEqual(SHELL.minHit - 1e-9);
    }
  });

  it('reserves the page gutters only once the hand actually pages', () => {
    // Reserving unconditionally would page a seven-card opening hand on phone
    // portrait, whose band is exactly wide enough for it without the gutters.
    const phone = shellBands({ width: 390, height: 844 }).hand.w;
    const opening = localFanPlan(7, phone);
    expect(opening.paged).toBe(false);
    expect(opening.span).toBe(phone);
    const deep = localFanPlan(20, phone);
    expect(deep.paged).toBe(true);
    expect(deep.span).toBe(phone - 2 * FAN.pagerW);
    expect(FAN.pagerW).toBeGreaterThanOrEqual(SHELL.minHit);
  });

  it('cannot oscillate: reserving the gutters never un-pages a hand', () => {
    for (const band of BAND_WIDTHS) {
      for (let count = 1; count <= 60; count += 1) {
        const { plan, paged } = localFanPlan(count, band);
        if (!paged) continue;
        expect(plan.pages).toBeGreaterThan(1);
      }
    }
  });

  it('covers every card exactly once across its pages', () => {
    for (const band of BAND_WIDTHS) {
      for (const count of [1, 7, 12, 13, 20, 21, 40, 60]) {
        const { plan } = localFanPlan(count, band);
        const seen: number[] = [];
        for (let page = 0; page < plan.pages; page += 1) {
          const { start, end } = fanPageRange(plan, page);
          expect(end).toBeGreaterThan(start);
          for (let i = start; i < end; i += 1) {
            seen.push(i);
            expect(fanPageOf(plan, i)).toBe(page);
          }
        }
        expect(seen).toEqual(Array.from({ length: count }, (_, i) => i));
      }
    }
  });

  it('reports no pages for an empty hand and never indexes into one', () => {
    const { plan } = localFanPlan(0, 882);
    expect(plan.pages).toBe(0);
    expect(fanPageRange(plan, 0)).toEqual({ start: 0, end: 0 });
    expect(fanPageOf(plan, 0)).toBe(0);
  });
});

describe('the fan is contained by the span it is given', () => {
  it('keeps both endpoints inside the span at every count and width', () => {
    for (const band of BAND_WIDTHS) {
      for (const count of [1, 2, 7, 12, 20, 40]) {
        const { plan, paged } = localFanPlan(count, band);
        const inset = fanInset(plan, LOCAL_FAN_TIER) + (paged ? FAN.pagerW : 0);
        const { start, end } = fanPageRange(plan, 0);
        const shown = end - start;
        for (let i = 0; i < shown; i += 1) {
          const centre = inset + (band - 2 * inset) * fanFraction(i, shown);
          expect(
            centre - LOCAL_FAN_TIER.card.w / 2,
            `band=${band} n=${count} i=${i}`,
          ).toBeGreaterThanOrEqual(-1e-9);
          expect(centre + LOCAL_FAN_TIER.card.w / 2).toBeLessThanOrEqual(band + 1e-9);
        }
      }
    }
  });

  it('never asks for more usable span than the band has', () => {
    for (const band of BAND_WIDTHS) {
      expect(fanUsableSpan(band, LOCAL_FAN_TIER)).toBeLessThan(band);
    }
  });
});

describe('the opponent tier', () => {
  it('scales its whole fan with the cluster rung, one D per fan', () => {
    const spans = [96, 76, 60, 48].map(opponentFanSpan);
    for (let i = 1; i < spans.length; i += 1) expect(spans[i]!).toBeLessThan(spans[i - 1]!);
    for (const d of [96, 76, 60, 48]) {
      const tier = opponentFanTier(d);
      expect(tier.minExposure).toBeGreaterThanOrEqual(4);
      expect(tier.minExposure).toBeLessThan(tier.card.w);
    }
  });

  it('bounds the drawn backs at every rung — the fan is not a per-card cost', () => {
    // The node-budget claim: an opponent fan's element count is bounded by its
    // geometry, never by how many cards the seat is holding.
    for (const d of [96, 76, 60, 48]) {
      const tier = opponentFanTier(d);
      const capacity = fanCapacity(opponentFanSpan(d), tier);
      expect(capacity).toBeLessThanOrEqual(16);
      for (const count of [0, 7, 20, 60, 200]) {
        const plan = fanPlan(count, opponentFanSpan(d), tier);
        expect(fanPageRange(plan, 0).end).toBeLessThanOrEqual(capacity);
      }
    }
  });
});
