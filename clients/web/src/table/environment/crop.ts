/**
 * Aspect handling for the environment plates (`docs/design/environment-system.md`
 * §4) — issue #530.
 *
 * L0–L2 are authored as one continuous 21:9 plate with the 16:9 safe crop
 * marked, so **ultrawide reveals** the outer 23.8 % of the plate rather than
 * stretching it. Every landscape crop is centred horizontally (§4.2): the
 * plaza's centre of mass is the source centre, so no per-theme anchor table is
 * needed and the same code path serves the SVG placeholder and the raster plate
 * (§10.2).
 *
 * L3 does not crop. Props are discrete anchored sprites (§4.4) — if they were
 * baked at fixed source coordinates the 16:9 crop would discard exactly the prop
 * pockets and 16:9 would have no visible scenery at all.
 */
import { ENV_AUTHORING_ASPECT, ENV_VIEWBOX } from './manifest';
import type { FractionRect } from './zones';

/** One row of the §4.2 crop table. */
export interface EnvCrop {
  /** The target aspect (`width / height`). */
  aspect: number;
  /** Fraction of the source plate's width the target uses. */
  sourceWidthFraction: number;
  /** The source rect the crop takes, in fractions of the plate. */
  source: FractionRect;
  /** The `viewBox` the crop resolves to on the 21:9 authoring canvas. */
  viewBox: string;
  /** Whether the crop is the §4.5 portrait recomposition rather than a crop. */
  recomposed: boolean;
}

/**
 * The tightest landscape crop of §4.3 — 4:3, using source `x ∈ [21.4 %, 78.6 %]`.
 * Everything load-bearing (the whole plaza field, the complete medallion, both
 * lips spanning the crop, the top and bottom paving rings) must be contained in
 * this rect; everything outside it is reveal-only surround.
 */
export const ENV_TIGHTEST_ASPECT = 4 / 3;

/** The aspect below which the §4.5 portrait recomposition takes over. */
export const ENV_PORTRAIT_ASPECT_CEILING = 1;

/**
 * The crop for a target aspect. A landscape aspect at or above the authoring
 * aspect uses the whole plate (ultrawide is a reveal, never a stretch); anything
 * narrower takes a centred slice `aspect / 2.333` wide at full height.
 *
 * Portrait is reported as `recomposed`: §4.5 replaces the crop with a cover-fit
 * L1 pinned so the medallion centre sits at `(50 %, 40 %)` of the viewport — the
 * same place it sits on desktop — because a 0.462 aspect would use 20 % of the
 * plate's width and show nothing recognisable.
 */
export function cropForAspect(aspect: number): EnvCrop {
  const recomposed = aspect < ENV_PORTRAIT_ASPECT_CEILING;
  const fraction = recomposed ? 1 : Math.min(1, aspect / ENV_AUTHORING_ASPECT);
  const x = (1 - fraction) / 2;
  const source: FractionRect = { x, y: 0, w: fraction, h: 1 };
  const viewBox = [
    (x * ENV_VIEWBOX.width).toFixed(2),
    '0',
    (fraction * ENV_VIEWBOX.width).toFixed(2),
    String(ENV_VIEWBOX.height),
  ].join(' ');
  return { aspect, sourceWidthFraction: fraction, source, viewBox, recomposed };
}

/** The crop for a viewport, guarding a zero/degenerate height. */
export function cropForViewport(viewport: { width: number; height: number }): EnvCrop {
  const aspect = viewport.height > 0 ? viewport.width / viewport.height : ENV_AUTHORING_ASPECT;
  return cropForAspect(aspect);
}

/**
 * Whether a source rect is **fully contained** in the tightest landscape crop
 * (§4.3) — the rule for an identity mark that must arrive whole or not at all:
 * the central rune medallion, and the top and bottom paving rings.
 */
export function containedInTightestCrop(source: FractionRect, epsilon = 1e-9): boolean {
  const crop = cropForAspect(ENV_TIGHTEST_ASPECT).source;
  return (
    source.x >= crop.x - epsilon &&
    source.x + source.w <= crop.x + crop.w + epsilon &&
    source.y >= -epsilon &&
    source.y + source.h <= 1 + epsilon
  );
}

/**
 * Whether a source rect **spans** the tightest landscape crop (§4.3) — the rule
 * for the two things that must reach the crop's edges rather than sit inside
 * them: the plaza field (otherwise a tablet shows unpaved ground under cards)
 * and both raised lips (they carry the entire depth read, so a lip that stops
 * short of the crop leaves the composition flat at that aspect).
 */
export function spansTightestCrop(source: FractionRect, epsilon = 1e-9): boolean {
  const crop = cropForAspect(ENV_TIGHTEST_ASPECT).source;
  return source.x <= crop.x + epsilon && source.x + source.w >= crop.x + crop.w - epsilon;
}
