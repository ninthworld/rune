/**
 * The **frame/face offset geometry** every gold-trimmed surface is built from
 * (issue #571, absorbed into #567).
 *
 * ## The defect this module exists to remove
 *
 * `control-language.md` §3.1 builds every framed surface as two boxes: an outer
 * box paints the 135° gold gradient and pads by the frame stroke, an inner
 * `.face` box paints the plate. A CSS `border` carries neither a gradient nor a
 * `clip-path`, so the trim has to be the outer box showing past the inner one.
 *
 * The shipped construction then clipped **both** boxes with the *same* polygon —
 * one custom property, one set of lengths, resolved once against the frame box
 * and once against the face box. For a rectangle that is fine. For a shape with
 * a diagonal it is not: the face box is `2 × stroke` shorter and narrower, so a
 * point depth stated as an absolute length lands on a different slope in the two
 * boxes and the polygons are no longer parallel.
 *
 * On the phase plaque (268 × 68, 22 px points, 2 px stroke) that put the trim at
 * **2.77 px** where the diagonal meets the flat top edge and **1.68 px** at the
 * leading point — the trim narrows into exactly the corner a reader looks at,
 * which is what the maintainer saw as a left trim "cut off rather than running to
 * the chevron's point". On the chamfered family the same error runs the other way:
 * a 45° cut shared by both boxes puts **2.83 px** of gold in each corner against
 * 2 px along the edges.
 *
 * ## The fix
 *
 * The face is clipped by the frame's outline **offset inward by the frame
 * stroke** — a true parallel offset — instead of by the same polygon re-resolved.
 * The two functions below are that offset in closed form; the values they produce
 * ship as `--rune-plaque-face-point`, `--rune-plaque-face-tip`, and
 * `--rune-control-chamfer-face` in `chrome/tokens.css`, and
 * {@link plaqueGeometry.test.ts} recomputes them from the declared source tokens
 * and fails if the stylesheet drifts from the arithmetic.
 *
 * Both are pure geometry over the tokens; nothing here reads the DOM, and jsdom
 * resolves no `clip-path`, so the drawn result is the maintainer's browser check.
 */
import { CONTROL } from './controlTokens';

/**
 * The face hexagon's point depth, measured from the **face box's own left edge**
 * to its top-left vertex.
 *
 * Derivation. Put the frame box at the origin with width `w`, height `h`, and
 * point depth `p`. Its upper-left edge runs from `(p, 0)` to `(0, h/2)`, i.e.
 * `(h/2)·x + p·y = (h/2)·p`, whose normal has length `L = √(p² + (h/2)²)`.
 * Offsetting that line inward by the stroke `t` gives
 * `(h/2)·x + p·y = (h/2)·p + t·L`. The face's top-left vertex is where that meets
 * the offset top edge `y = t`, so in frame coordinates
 * `x = p + t·(L − p)/(h/2)`; the face box itself starts at `x = t`, so the depth
 * the polygon needs is that minus `t`.
 */
export function facePointDepth(
  point: number = CONTROL.plaquePoint,
  height: number = CONTROL.plaqueH,
  stroke: number = CONTROL.frameW,
): number {
  const half = height / 2;
  const diagonal = Math.hypot(point, half);
  return point + (stroke * (diagonal - point)) / half - stroke;
}

/**
 * How far the face hexagon's leading tip sits inside the **face box's own left
 * edge**.
 *
 * Same offset line as {@link facePointDepth}, met with the vertical centre line
 * `y = h/2`: in frame coordinates `x = 2·t·L/h`, and the face box starts at
 * `x = t`. It is a fraction of a pixel — but it is the difference between a trim
 * that thins into the point and one that runs around it, because without it the
 * two diagonals converge at different rates.
 */
export function faceTipInset(
  point: number = CONTROL.plaquePoint,
  height: number = CONTROL.plaqueH,
  stroke: number = CONTROL.frameW,
): number {
  const diagonal = Math.hypot(point, height / 2);
  return (2 * stroke * diagonal) / height - stroke;
}

/**
 * The face's 45° corner cut, for the chamfered family (`controls.module.css`,
 * and the pregame's `Plaque`, which shares the construction verbatim).
 *
 * A chamfer edge is `x + y = c`. The face box is inset by `t` on both axes, so
 * clipping it with the same `c` puts its edge at `x + y = c + 2t` in frame
 * coordinates — a perpendicular gap of `2t/√2 = t·√2`, i.e. 41 % more gold in
 * every corner than along the edges. A true inward offset wants
 * `x + y = c + t·√2`, so the face's own cut is `c − t·(2 − √2)`.
 */
export function faceChamfer(
  chamfer: number = CONTROL.chamfer,
  stroke: number = CONTROL.frameW,
): number {
  return chamfer - stroke * (2 - Math.SQRT2);
}

/**
 * The token in `chrome/tokens.css` each derived length ships as, with the
 * function that produces it. The drift guard iterates this rather than naming the
 * pairs twice.
 */
export const FACE_TOKEN_NAMES: Record<string, () => number> = {
  '--rune-plaque-face-point': () => facePointDepth(),
  '--rune-plaque-face-tip': () => faceTipInset(),
  '--rune-control-chamfer-face': () => faceChamfer(),
};
