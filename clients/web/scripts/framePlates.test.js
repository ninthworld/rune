import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import {
  authoringScale,
  PLATE_FRAME,
  PLATE_GOLD,
  PLATE_SPECS,
  renderPlate,
} from './framePlates.js';
import { RUNE_FRAME, RUNE_GOLD } from '../src/tokens';

const manifest = JSON.parse(
  readFileSync(
    resolve(dirname(fileURLToPath(import.meta.url)), '../public/assets/manifest.json'),
    'utf8',
  ),
);
const plates = manifest.cardFrames.plates;

describe('card-frame plate synthesis (issue #570)', () => {
  it('is authored against the live frame tokens, not a stale copy of them', () => {
    // The plates bake geometry: the gold hairline lands on `ruleInset` and
    // nowhere else. A token change that this copy did not follow would ship a
    // frame whose material and whose CSS disagree, and the only way to see it
    // would be to look at a card — so it fails here instead.
    expect(PLATE_FRAME).toEqual({
      radius: RUNE_FRAME.radius,
      plateRadius: RUNE_FRAME.plateRadius,
      edge: RUNE_FRAME.edge,
      edgeBottom: RUNE_FRAME.edgeBottom,
      rule: RUNE_FRAME.rule,
      ruleInset: RUNE_FRAME.ruleInset,
    });
    expect(PLATE_GOLD).toEqual({ rule: RUNE_GOLD.rule, ruleShade: RUNE_GOLD.ruleShade });
  });

  it('keeps every sliced plate nine-slice safe', () => {
    for (const spec of PLATE_SPECS.filter((candidate) => candidate.slice > 0)) {
      const k = authoringScale(spec);
      // The corner must fit inside its slice. Otherwise the curve spills into
      // the edge patches, which `border-image` stretches along their axis — and
      // a stretched curve is a bent corner at every tier but the authored one.
      expect(spec.radius * k, spec.id).toBeLessThan(spec.slice);
      // A positive middle patch, so the stretched region is real geometry
      // rather than a resampled single row or column.
      expect(spec.width - spec.slice * 2, spec.id).toBeGreaterThan(0);
      expect(spec.height - spec.slice * 2, spec.id).toBeGreaterThan(0);
    }
  });

  it("spans the frame edge from the card's boundary to the hairline's inner lip", () => {
    // Not a coincidence to be re-derived in CSS: the band IS `ruleInset + rule`,
    // which is what lets the plate hand off to the card body with no seam.
    const edge = PLATE_SPECS.find((spec) => spec.key === 'frameEdge');
    expect(edge.band).toBeCloseTo(RUNE_FRAME.ruleInset + RUNE_FRAME.rule, 10);
  });

  it('renders deterministically — the same spec, the same bytes', () => {
    // The content hash in every filename is only meaningful if a re-run
    // reproduces the pixels; the synthesis therefore uses integer hashing
    // rather than a PRNG, and this is the assertion that keeps it that way.
    for (const spec of PLATE_SPECS) {
      const first = renderPlate(spec);
      const second = renderPlate(spec);
      expect(Buffer.from(first.rgba).equals(Buffer.from(second.rgba)), spec.id).toBe(true);
    }
  });

  it('produces alpha light maps — material, never a body colour', () => {
    // The plates carry bevel, shadow, and grain over the token fills beneath
    // them. Only the frame edge carries a hue at all, and only the structural
    // gold hairline (§3.4 keeps gold structural and identity an accent), so one
    // set serves both environment themes and all eight colour identities.
    for (const spec of PLATE_SPECS.filter((candidate) => candidate.key !== 'frameEdge')) {
      const { rgba } = renderPlate(spec);
      for (let i = 0; i < rgba.length; i += 4) {
        if (rgba[i + 3] === 0) continue;
        expect(rgba[i] === rgba[i + 1] && rgba[i + 1] === rgba[i + 2], spec.id).toBe(true);
      }
    }
  });

  it('ships exactly the specified plates, with the manifest the generator wrote', () => {
    // Byte equality with the committed WebP is not asserted: the shipping
    // encode runs through cwebp/ImageMagick, which is not a client dependency.
    // The structural contract is, and it is what the stylesheet resolves
    // against — a spec edited without re-running the generator fails here.
    expect(Object.keys(plates).sort()).toEqual(PLATE_SPECS.map((spec) => spec.key).sort());
    for (const spec of PLATE_SPECS) {
      expect(plates[spec.key], spec.id).toMatchObject({
        width: spec.width,
        height: spec.height,
        slice: spec.slice,
        band: spec.band,
        fill: spec.fill,
        // The frame is on every card, so no plate may be deferred behind the
        // first-match gate — a lazily-loaded frame is a frameless first match.
        load: 'first-match',
      });
      expect(plates[spec.key].src).toMatch(
        new RegExp(`^/assets/frames/${spec.id}\\.[a-f0-9]{8}\\.(?:webp|png)$`),
      );
    }
  });
});
