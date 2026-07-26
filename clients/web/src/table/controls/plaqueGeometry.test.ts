/**
 * The frame/face offset guard (issue #571, absorbed into #567).
 *
 * `plaqueGeometry.ts` derives three lengths that ship as tokens, so the same
 * duplication `controlTokens.test.ts` guards applies here: a derived value that
 * exists in a stylesheet and in a formula is a value that will eventually be
 * edited in one of them. This recomputes the formula from the *source* tokens
 * (`--rune-plaque-point`, `--rune-plaque-h`, `--rune-control-chamfer`,
 * `--rune-control-frame-w`) and fails if the declared result drifts.
 *
 * It also pins the property that made the trim uneven in the first place: the
 * offset outline is *parallel* to the frame's, so the perpendicular gap between
 * them is the frame stroke everywhere rather than only on the flat edges.
 * jsdom resolves no `clip-path` and computes no layout, so nothing here claims a
 * drawn pixel — the rendered trim is the maintainer's browser check.
 */
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { CONTROL } from './controlTokens';
import { FACE_TOKEN_NAMES, faceChamfer, facePointDepth, faceTipInset } from './plaqueGeometry';

function css(relative: string): string {
  return readFileSync(fileURLToPath(new URL(relative, import.meta.url)), 'utf8');
}

/** The declared value of a `--token: <n>px;` declaration, as a number. */
function declared(sheet: string, token: string): number {
  const match = new RegExp(`${token}:\\s*([\\d.]+)px;`).exec(sheet);
  expect(match, `${token} is not declared as a px length`).not.toBeNull();
  return Number(match![1]);
}

/**
 * The perpendicular distance from the frame hexagon's upper-left edge to a point
 * on the face's, in frame coordinates. The frame edge runs `(p, 0) → (0, h/2)`,
 * i.e. `(h/2)x + p·y = (h/2)p`.
 */
function gapToFrameDiagonal(x: number, y: number): number {
  const p = CONTROL.plaquePoint;
  const half = CONTROL.plaqueH / 2;
  return (half * x + p * y - half * p) / Math.hypot(half, p);
}

describe('frame/face offset geometry', () => {
  const tokens = css('../../chrome/tokens.css');

  it('ships every derived length as the token its formula produces', () => {
    for (const [token, derive] of Object.entries(FACE_TOKEN_NAMES)) {
      expect(declared(tokens, token), `${token} drifted from its derivation`).toBeCloseTo(
        derive(),
        3,
      );
    }
  });

  it('offsets the plaque hexagon by exactly one frame stroke at the leading point', () => {
    // The defect: the shipped face reused the frame's polygon, which put this
    // gap at 1.68px at the point and 2.77px at the plate's corner — the trim
    // narrowing into the very corner it should run around.
    const stroke = CONTROL.frameW;
    const tip = { x: stroke + faceTipInset(), y: CONTROL.plaqueH / 2 };
    const corner = { x: stroke + facePointDepth(), y: stroke };

    expect(gapToFrameDiagonal(tip.x, tip.y)).toBeCloseTo(stroke, 6);
    expect(gapToFrameDiagonal(corner.x, corner.y)).toBeCloseTo(stroke, 6);
  });

  it('keeps the face outline inside the frame and its points shallower', () => {
    // A face point deeper than the frame's, or a tip outside it, would put the
    // plate through the trim rather than inside it.
    expect(faceTipInset()).toBeGreaterThan(0);
    expect(facePointDepth()).toBeLessThan(CONTROL.plaquePoint);
    expect(faceChamfer()).toBeLessThan(CONTROL.chamfer);
  });

  it('offsets the chamfered family by exactly one frame stroke at the corner', () => {
    // A 45° edge `x + y = c` in the face box sits at `x + y = c + 2t` in frame
    // coordinates; the frame's own is `x + y = c`. The perpendicular gap is the
    // difference over √2, and it has to equal the stroke, not 1.41 times it.
    const stroke = CONTROL.frameW;
    const faceEdgeInFrameCoords = faceChamfer() + 2 * stroke;
    expect((faceEdgeInFrameCoords - CONTROL.chamfer) / Math.SQRT2).toBeCloseTo(stroke, 6);
  });

  it('scales with the tokens rather than pinning the shipped numbers', () => {
    // Growing the stroke grows the inset; growing the plate's height makes the
    // diagonals shallower and the point correction smaller. Both follow from the
    // formula, and both are what a hard-coded polygon would get wrong.
    expect(facePointDepth(22, 68, 4)).toBeLessThan(facePointDepth(22, 68, 2));
    expect(faceTipInset(22, 68, 4)).toBeGreaterThan(faceTipInset(22, 68, 2));
    expect(faceTipInset(22, 200, 2)).toBeLessThan(faceTipInset(22, 68, 2));
  });
});
