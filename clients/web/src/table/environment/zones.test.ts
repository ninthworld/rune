/**
 * The §2.5 focal-safe geometry gates of `docs/design/environment-system.md` —
 * issue #530.
 *
 * The document is explicit that these are "pure-geometry assertions and belong
 * in unit tests, not in a browser", and lists exactly four:
 *
 * 1. Every prop anchor in the theme manifest lies inside Zone C (or Zone B for
 *    `mass: 'low'` anchors).
 * 2. Zone A ∩ (L2 anchors ∪ L3 anchors) = ∅ except anchors tagged `lip`.
 * 3. For every seat count 2…6, `carveSlots()`'s union of rects is contained in
 *    Zone A ∪ Zone B.
 * 4. The medallion centre and radius match §2.3 at every supported aspect.
 *
 * Assertion 3 is the load-bearing one for the layout boundary of §3.3: it runs
 * against the **live `carveSlots`**, so a plane-geometry change that widens the
 * occupied union fails here rather than silently invalidating the art.
 *
 * What jsdom cannot show, and what this file therefore does not claim: nothing
 * here proves a rendered pixel, a real layout, or an actual contrast reading on
 * screen. These are assertions about the declared geometry contract.
 */
import { describe, expect, it } from 'vitest';
import {
  ENV_AMBIENT_SPACE,
  ENV_LIP_BANDS,
  ENV_MEDALLION,
  ENV_MID_ANCHOR_PINCH,
  ENV_PROP_ANCHORS,
  ENV_ZONES,
  bottom,
  clipToCanvas,
  containsRect,
  inAmbientSpace,
  inFocalCore,
  inPropPocket,
  inSeatFlank,
  intersectsFocalCore,
  overlapsRect,
  propPlacementIsLegal,
  propRect,
  right,
  unionRect,
  withinLipCarveOut,
  withinSeatEnvelope,
  zoneOf,
  type FractionRect,
} from './zones';
import {
  ENV_MANIFESTS,
  ambientReservationIsQuiet,
  propFootprint,
  type EnvThemeManifest,
} from './manifest';
import {
  ENV_REFERENCE_VIEWPORTS,
  ENV_SEAT_COUNTS,
  planeOccupancy,
  toFractionRect,
} from './planeOccupancy';
import { containedInTightestCrop, cropForAspect, spansTightestCrop } from './crop';

const THEMES = Object.values(ENV_MANIFESTS) as EnvThemeManifest[];

describe('environment zones — §2.2 the normative zones', () => {
  it('places the focal-safe rectangle at the central 80 % of width, full height', () => {
    // "The answer in one line" of §2.2. These rects are theme-invariant (§5.1),
    // so a theme may never move them.
    expect(ENV_ZONES.focalCore).toEqual({ x: 0.1, y: 0, w: 0.8, h: 1 });
  });

  it('places the seat flanks at x 0–10 % / 90–100 %, y 10–67 %', () => {
    expect(ENV_ZONES.seatFlanks).toHaveLength(2);
    for (const flank of ENV_ZONES.seatFlanks) {
      expect(flank.w).toBeCloseTo(0.1, 10);
      expect(flank.y).toBeCloseTo(0.1, 10);
      expect(bottom(flank)).toBeCloseTo(0.67, 10);
    }
    expect(ENV_ZONES.seatFlanks[0]!.x).toBe(0);
    expect(right(ENV_ZONES.seatFlanks[1]!)).toBeCloseTo(1, 10);
  });

  it('leaves four prop pockets totalling 8.6 % of canvas area', () => {
    expect(ENV_ZONES.propPockets).toHaveLength(4);
    const area = ENV_ZONES.propPockets.reduce((sum, r) => sum + r.w * r.h, 0);
    expect(area).toBeCloseTo(0.086, 3);
  });

  it('never overlaps a zone with another zone', () => {
    const all = [ENV_ZONES.focalCore, ...ENV_ZONES.seatFlanks, ...ENV_ZONES.propPockets];
    for (let i = 0; i < all.length; i += 1) {
      for (let j = i + 1; j < all.length; j += 1) {
        expect(overlapsRect(all[i]!, all[j]!)).toBe(false);
      }
    }
  });

  it('classifies a rect into exactly one zone, or reports the straddle', () => {
    expect(zoneOf({ x: 0.4, y: 0.4, w: 0.2, h: 0.2 })).toBe('A');
    expect(zoneOf({ x: 0.01, y: 0.2, w: 0.05, h: 0.2 })).toBe('B');
    expect(zoneOf({ x: 0.01, y: 0.01, w: 0.05, h: 0.05 })).toBe('C');
    // A rect crossing the core boundary belongs to no single zone.
    expect(zoneOf({ x: 0.05, y: 0.2, w: 0.2, h: 0.2 })).toBeUndefined();
  });
});

describe('environment zones — §2.5 assertion 1: every prop anchor is legally placed', () => {
  it('confines full-mass props to Zone C and low-mass props to Zone B or C', () => {
    for (const manifest of THEMES) {
      for (const prop of manifest.props) {
        const rect = propFootprint(prop);
        const zone = zoneOf(rect);
        expect({ theme: manifest.theme, prop: prop.key, zone }).toEqual({
          theme: manifest.theme,
          prop: prop.key,
          zone: prop.mass === 'full' ? 'C' : zone,
        });
        if (prop.mass === 'full') expect(zone).toBe('C');
        else expect(zone === 'B' || zone === 'C').toBe(true);
        expect(propPlacementIsLegal(prop.anchor, rect, prop.mass)).toBe(true);
      }
    }
  });

  it('limits the two mid anchors to low mass inside the §4.4 pinch', () => {
    for (const manifest of THEMES) {
      for (const prop of manifest.props) {
        if (prop.anchor !== 'left-mid' && prop.anchor !== 'right-mid') continue;
        const rect = propFootprint(prop);
        expect(prop.mass).toBe('low');
        if (prop.anchor === 'left-mid')
          expect(right(rect)).toBeLessThanOrEqual(ENV_MID_ANCHOR_PINCH);
        else expect(rect.x).toBeGreaterThanOrEqual(1 - ENV_MID_ANCHOR_PINCH);
      }
    }
  });

  it('rejects a prop that would sit in the focal core, or a tall one at a mid anchor', () => {
    // The inverse direction: the predicate is a real gate, not a tautology.
    expect(propPlacementIsLegal('top-left', { x: 0.3, y: 0.02, w: 0.1, h: 0.05 }, 'full')).toBe(
      false,
    );
    expect(propPlacementIsLegal('left-mid', { x: 0.0, y: 0.3, w: 0.03, h: 0.1 }, 'full')).toBe(
      false,
    );
    // A low-mass prop that escapes the pinch is rejected even inside Zone B.
    expect(propPlacementIsLegal('left-mid', { x: 0.0, y: 0.3, w: 0.09, h: 0.1 }, 'low')).toBe(
      false,
    );
  });

  it('uses only the six §4.4 anchor names — a theme may leave one empty, never add one', () => {
    for (const manifest of THEMES) {
      for (const prop of manifest.props) {
        expect(ENV_PROP_ANCHORS).toContain(prop.anchor);
      }
      expect(new Set(manifest.props.map((p) => p.key)).size).toBe(manifest.props.length);
    }
  });

  it('anchors props in canvas fractions, so 16:9 and 21:9 place them identically', () => {
    // §4.4's whole point. A baked plate would lose every prop at the 16:9 crop,
    // because the prop pockets sit outside source x ∈ [11.9 %, 88.1 %].
    const wide = cropForAspect(21 / 9);
    expect(wide.source.x).toBeLessThan(0.12);
    const standard = cropForAspect(16 / 9);
    expect(standard.source.x).toBeCloseTo(0.119, 2);
    // The prop pockets are outside the 16:9 source window…
    expect(ENV_ZONES.propPockets[0]!.w).toBeLessThan(standard.source.x);
    // …yet every prop still resolves to the same canvas rect, because the anchor
    // is canvas-relative and never consults the crop.
    for (const prop of ENV_MANIFESTS.runicVale.props) {
      expect(propRect(prop.anchor, prop.offset, prop.size)).toEqual(propFootprint(prop));
    }
  });
});

describe('environment zones — §2.5 assertion 2: Zone A holds no L2 or L3 but the lips', () => {
  it('keeps every L3 prop out of the focal core entirely', () => {
    for (const manifest of THEMES) {
      for (const prop of manifest.props) {
        // §2.4: "L3 has no carve-out and never enters Zone A at any height."
        expect(intersectsFocalCore(propFootprint(prop))).toBe(false);
      }
    }
  });

  it('admits L2 into the focal core only inside the two lip bands', () => {
    expect(ENV_LIP_BANDS).toHaveLength(2);
    for (const band of ENV_LIP_BANDS) {
      expect(band.h).toBeCloseTo(0.08, 10);
      expect(withinLipCarveOut(band)).toBe(true);
      // A lip crosses Zone A by construction — that is the carve-out.
      expect(intersectsFocalCore(band)).toBe(true);
    }
    expect(ENV_LIP_BANDS[0]!.y).toBe(0);
    expect(bottom(ENV_LIP_BANDS[1]!)).toBeCloseTo(1, 10);
    // Anything taller than 8 % of H, or anywhere else, is not a lip.
    expect(withinLipCarveOut({ x: 0, y: 0, w: 1, h: 0.12 })).toBe(false);
    expect(withinLipCarveOut({ x: 0, y: 0.4, w: 1, h: 0.05 })).toBe(false);
  });
});

describe('environment zones — §2.5 assertion 3: the plane occupies only Zone A ∪ Zone B', () => {
  // The layout boundary of §3.3 made executable. `carveSlots` is imported and
  // called; nothing here mutates plane geometry (that belongs to issue #531).
  //
  // Scope note: `carveSlots` produces the receiver band, the far side, the
  // wings, and the corridor. Crest and pile clusters are attached later by
  // `stagePlane` and are NOT covered — see the receiver-crest finding reported
  // with this issue.
  for (const { label, viewport } of ENV_REFERENCE_VIEWPORTS) {
    for (const count of ENV_SEAT_COUNTS) {
      it(`contains every carved slot at ${count} seats on ${label}`, () => {
        const rects = planeOccupancy(count, viewport);
        expect(rects.length).toBeGreaterThan(0);
        for (const rect of rects) {
          expect({ label, count, rect, inside: withinSeatEnvelope(rect) }).toEqual({
            label,
            count,
            rect,
            inside: true,
          });
        }
      });
    }
  }

  it('keeps the union clear of the four prop pockets at every seat count', () => {
    // Zone C is the entire budget for illustrated incident. If a slot ever
    // reached into a pocket, the art would be occluded rather than framed.
    for (const { viewport } of ENV_REFERENCE_VIEWPORTS) {
      for (const count of ENV_SEAT_COUNTS) {
        for (const rect of planeOccupancy(count, viewport)) {
          for (const pocket of ENV_ZONES.propPockets) {
            expect(overlapsRect(rect, pocket)).toBe(false);
          }
        }
      }
    }
  });

  it('detects a rect that escapes the envelope (the check is not vacuous)', () => {
    // A receiver band widened to x 6–94 % — the compact branch's geometry —
    // reaches below the flanks' 67 % floor outside the core and is rejected.
    expect(withinSeatEnvelope({ x: 0.06, y: 0.6, w: 0.88, h: 0.4 })).toBe(false);
    expect(withinSeatEnvelope({ x: 0.12, y: 0.67, w: 0.76, h: 0.33 })).toBe(true);
  });

  it('clips a wing’s offstage bleed before zoning, as §2.1 specifies', () => {
    // The left wing is staged at x = -0.28 × its width; the union is taken after
    // the clip, so the negative x never leaks into the derivation.
    const clipped = clipToCanvas({ x: -0.0672, y: 0.12, w: 0.24, h: 0.4 });
    expect(clipped.x).toBe(0);
    expect(right(clipped)).toBeCloseTo(0.1728, 6);
  });
});

describe('environment zones — §2.5 assertion 4: the medallion at every aspect', () => {
  it('centres the medallion at (50 %, 40 %) with r = 5 % of width', () => {
    expect(ENV_MEDALLION).toEqual({ cx: 0.5, cy: 0.4, r: 0.05 });
  });

  it('keeps the medallion inside the focal core and inside the corridor band', () => {
    const rect: FractionRect = {
      x: ENV_MEDALLION.cx - ENV_MEDALLION.r,
      y: ENV_MEDALLION.cy - ENV_MEDALLION.r,
      w: ENV_MEDALLION.r * 2,
      h: ENV_MEDALLION.r * 2,
    };
    expect(inFocalCore(rect)).toBe(true);
    // §2.3: it is the only permitted L1 incident inside Zone A, and it sits in
    // the centre corridor, which by construction holds no card.
    expect(rect.x).toBeCloseTo(0.45, 10);
    expect(right(rect)).toBeCloseTo(0.55, 10);
  });

  it('survives the tightest landscape crop whole, at every supported aspect', () => {
    // §4.3: the medallion is "the theme's single readable identity mark" and
    // must arrive complete or not at all. Its half-width in SOURCE fractions
    // grows as the crop narrows, which is why this is checked at the tightest.
    for (const aspect of [21 / 9, 16 / 9, 16 / 10, 3 / 2, 1180 / 820, 4 / 3]) {
      const crop = cropForAspect(aspect);
      const halfWidthInSource = ENV_MEDALLION.r * crop.sourceWidthFraction;
      const inSource: FractionRect = {
        x: 0.5 - halfWidthInSource,
        y: ENV_MEDALLION.cy - ENV_MEDALLION.r,
        w: halfWidthInSource * 2,
        h: ENV_MEDALLION.r * 2,
      };
      expect(containedInTightestCrop(inSource)).toBe(true);
      // And the crop itself is always centred (§4.2 — one anchor, every theme).
      expect(crop.source.x + crop.source.w / 2).toBeCloseTo(0.5, 10);
    }
  });

  it('requires the plaza and both lips to SPAN the tightest crop, not sit inside it', () => {
    // The other half of §4.3: the plaza must reach the crop's edges (otherwise a
    // tablet shows unpaved ground under cards) and both lips must span its
    // width (they carry the entire depth read).
    const fullWidth: FractionRect = { x: 0, y: 0, w: 1, h: 1 };
    expect(spansTightestCrop(fullWidth)).toBe(true);
    for (const band of ENV_LIP_BANDS) expect(spansTightestCrop(band)).toBe(true);
    // A plaza that stopped at the 16:9 window would fail at 4:3.
    expect(spansTightestCrop({ x: 0.25, y: 0, w: 0.5, h: 1 })).toBe(false);
  });
});

describe('environment zones — §6 the AMBIENT SPACE reservation', () => {
  it('reserves x 0–20 %, y 69–97 % with the chamfer above y = 80 %', () => {
    expect(ENV_AMBIENT_SPACE.rect).toEqual({ x: 0, y: 0.69, w: 0.2, h: 0.28 });
    expect(ENV_AMBIENT_SPACE.chamferY).toBeCloseTo(0.8, 10);
    expect(ENV_AMBIENT_SPACE.chamferX).toBeCloseTo(0.12, 10);
    // Below the chamfer line the full 20 % is available…
    expect(inAmbientSpace({ x: 0.02, y: 0.7, w: 0.16, h: 0.05 })).toBe(true);
    // …above it the inboard edge pulls back to 12 %.
    expect(inAmbientSpace({ x: 0.02, y: 0.85, w: 0.16, h: 0.05 })).toBe(false);
    expect(inAmbientSpace({ x: 0.02, y: 0.85, w: 0.08, h: 0.05 })).toBe(true);
  });

  it('names the subregion no seat contests at any count (§6.3)', () => {
    // The bottom-left prop pocket, which also carries the in-match wordmark.
    expect(ENV_AMBIENT_SPACE.uncontested).toEqual({ x: 0, y: 0.69, w: 0.1, h: 0.31 });
    expect(inPropPocket({ x: 0, y: 0.7, w: 0.09, h: 0.2 })).toBe(true);
  });

  it('keeps the region quiet: at most one addressable prop, in every theme (§6.2)', () => {
    for (const manifest of THEMES) {
      expect(ambientReservationIsQuiet(manifest)).toBe(true);
      const inside = manifest.props.filter((prop) => inAmbientSpace(propFootprint(prop)));
      expect(inside.length).toBeLessThanOrEqual(1);
      for (const prop of inside) {
        // Independently addressable, so claiming the region hides exactly one
        // thing, and no taller than 12 % of H.
        expect(prop.ambient).toBe(true);
        expect(prop.size.h).toBeLessThanOrEqual(0.12);
      }
    }
  });
});

describe('environment zones — rect algebra', () => {
  it('unions and clips rects the way §2.1 derives the envelope', () => {
    expect(unionRect([])).toBeUndefined();
    expect(
      unionRect([
        { x: 0.1, y: 0.2, w: 0.2, h: 0.2 },
        { x: 0.5, y: 0.1, w: 0.2, h: 0.5 },
      ]),
    ).toEqual({ x: 0.1, y: 0.1, w: 0.6, h: 0.5 });
    expect(clipToCanvas({ x: -0.5, y: -0.5, w: 2, h: 2 })).toEqual({ x: 0, y: 0, w: 1, h: 1 });
    expect(containsRect(ENV_ZONES.focalCore, { x: 0.05, y: 0, w: 0.1, h: 0.1 })).toBe(false);
    expect(inSeatFlank({ x: 0.92, y: 0.2, w: 0.05, h: 0.2 })).toBe(true);
  });

  it('normalises a plane rect against the viewport it was carved for', () => {
    expect(toFractionRect({ x: 128, y: 72, w: 256, h: 144 }, { width: 1280, height: 720 })).toEqual(
      {
        x: 0.1,
        y: 0.1,
        w: 0.2,
        h: 0.2,
      },
    );
  });
});
