import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { basename, dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { inflateSync } from 'node:zlib';
import { describe, expect, it } from 'vitest';
import {
  authoringScale,
  encodePng,
  PLATE_FRAME,
  PLATE_GOLD,
  PLATE_SPECS,
  plateFilename,
  renderPlate,
} from './framePlates.js';
import { RUNE_FRAME, RUNE_GOLD } from '../src/tokens';

const clientRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const framesDir = resolve(clientRoot, 'public/assets/frames');
const manifest = JSON.parse(
  readFileSync(resolve(clientRoot, 'public/assets/manifest.json'), 'utf8'),
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

  it('regenerates to the committed bytes — `npm run frames` is a checked no-op', () => {
    // The end-to-end reproducibility check, and the reason the shipping encoder
    // is in-process rather than a probe for cwebp/ImageMagick: two contributors
    // running the documented command must leave the repository in the same
    // state, and that is only a claim worth making if something verifies it.
    //
    // This re-runs the whole pipeline — synthesis, PNG encode, content hash,
    // filename — and compares against what is actually committed. Any drift,
    // whether in the drawing code, the encoder, or the zlib build underneath
    // it, fails here instead of silently rewriting seven assets on someone
    // else's machine.
    for (const spec of PLATE_SPECS) {
      const { width, height, rgba } = renderPlate(spec);
      const bytes = encodePng(width, height, rgba);
      const hash = createHash('sha256').update(bytes).digest('hex').slice(0, 8);
      const file = plateFilename(spec, hash);
      expect(plates[spec.key].src, spec.id).toBe(`/assets/frames/${file}`);
      expect(bytes.equals(readFileSync(resolve(framesDir, file))), spec.id).toBe(true);
    }
  });

  it('emits a DEFLATE stream a standard inflater accepts', () => {
    // The compressor is this repository's own (fixed Huffman over greedy LZ77),
    // because a content hash that depends on the machine's zlib is not a
    // content hash — CI proved that the hard way. Writing the compressor means
    // owning its correctness, so every committed plate is inflated back with
    // Node's zlib and checked against the exact scanline layout PNG requires.
    for (const spec of PLATE_SPECS) {
      const committed = readFileSync(resolve(framesDir, basename(plates[spec.key].src)));
      const idat = [];
      for (let at = 8; at < committed.length - 8;) {
        const length = committed.readUInt32BE(at);
        const type = committed.toString('ascii', at + 4, at + 8);
        if (type === 'IDAT') idat.push(committed.subarray(at + 8, at + 8 + length));
        at += 12 + length;
      }
      const raw = inflateSync(Buffer.concat(idat));
      const samples = spec.key === 'frameEdge' ? 4 : 2;
      expect(raw.length, spec.id).toBe((spec.width * samples + 1) * spec.height);
      // Every scanline opens with one of the five PNG filter types.
      for (let y = 0; y < spec.height; y += 1) {
        expect(raw[y * (spec.width * samples + 1)], `${spec.id} row ${y}`).toBeLessThan(5);
      }
    }
  });

  it('ships greyscale plates as greyscale, colour only where a hue is carried', () => {
    // PNG colour type 4 halves the samples of a light map. Only the frame edge,
    // which carries the structural gold hairline, needs type 6 — so this is
    // also a second guard on the light-map property asserted below.
    for (const spec of PLATE_SPECS) {
      const committed = readFileSync(resolve(framesDir, basename(plates[spec.key].src)));
      // IHDR colour type: byte 25 of a PNG (8 signature + 8 chunk header + 9).
      expect(committed[25], spec.id).toBe(spec.key === 'frameEdge' ? 6 : 4);
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
    // The structural half of the contract, and what the stylesheet resolves
    // against — a spec edited without re-running the generator fails here as
    // well as in the byte-equality check above.
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
        new RegExp(`^/assets/frames/${spec.id}\\.[a-f0-9]{8}\\.png$`),
      );
    }
  });
});
