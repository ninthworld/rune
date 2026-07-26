/**
 * The card frame's **plates** (issue #570) projected into CSS custom properties.
 *
 * Until now the frame had no generated material behind it: the stylesheet
 * approximated an edge with a border, a bevel with a box-shadow, and a printed
 * surface with a flat fill, so a card could only ever read as a light rectangle
 * with lines on it. `card-representation.md` §3.12 replaces those
 * approximations with seven bundled plates, and this module is the only place
 * that turns the shipped manifest into something the stylesheet can read.
 *
 * Three rules hold everything together:
 *
 * - **The plates are alpha light maps.** They carry highlight, shadow, grain,
 *   and the structural gold hairline — never a body colour. Every fill still
 *   comes from `src/tokens.ts` through `theme.ts` and shows *through* the
 *   plate, so ADR 0019 is untouched, both environment themes are served by one
 *   set, and colour identity keeps a material instead of becoming the flat
 *   colour block §3.4 forbids.
 * - **The band is a ratio of `W`, never a fixed inset.** Each plate declares
 *   the fraction of the card width its nine-slice band occupies; resolving it
 *   per tier is what lets ONE asset serve the hand fan, the inspect panel, and
 *   the battlefield chip, in all three silhouettes, exactly as the tokens'
 *   other ratios do.
 * - **A plate is never load-bearing, and the carriers cost no layout.** Every
 *   rule that consumes a plate keeps its token treatment underneath — the gold
 *   hairline is still a real border, the parchment still a real background, the
 *   rims still box-shadows — and every value published here reaches the
 *   stylesheet through `border-image-*` or `background-image`/`-size`/`-blend`,
 *   which are paint-only. The carriers declare `border-width: 0` and give
 *   `border-image-width` an explicit length, which is independent of the border
 *   width (it is a multiple of it only when given as a `<number>`), so the image
 *   overflows inward into the padding box while the border itself occupies
 *   nothing. Consequently a plate that 404s, a browser that declines
 *   `border-image`, and a build with no frames tree at all produce the SAME
 *   boxes, line heights, and crops as the frame rendered before this set
 *   landed. `plates.test.ts` asserts that mechanically rather than by
 *   inspection: no plate property may appear outside the paint-only set.
 *
 * Paths and slices come from the committed manifest — never transcribed here —
 * for the reason `assets/productionManifest.ts` sets out: a hash changes on
 * every regeneration, and a hash in a `.ts` file is a breakage the type system
 * cannot catch.
 */
import { PRODUCTION_FRAME_PLATES } from '../../assets/productionManifest';

/** The manifest keys this module composes, in the order the frame stacks them. */
const FRAME_EDGE = 'frameEdge';
const ART_SEAM = 'artSeam';
const HEADER_FIELD = 'headerField';
const INFO_STRIP = 'infoStrip';
const STATUS_STRIP = 'statusStrip';
const PT_PLATE = 'ptPlate';
const IDENTITY_WEAVE = 'identityWeave';

/** A plate's `src` as a CSS `url()`, or `none` when the key is not shipped. */
function source(key: string): string {
  const plate = PRODUCTION_FRAME_PLATES[key];
  return plate === undefined ? 'none' : `url(${plate.src})`;
}

/** A plate's nine-slice inset as a unitless `border-image-slice` value. */
function slice(key: string): string {
  return `${PRODUCTION_FRAME_PLATES[key]?.slice ?? 0}`;
}

/** A plate's band as a fraction of the card width `W`. */
export function bandRatio(key: string): number {
  return PRODUCTION_FRAME_PLATES[key]?.band ?? 0;
}

/**
 * The tier-independent half: which plate each surface draws and how it is
 * sliced. Frozen once, exactly like `theme.ts`'s `MATERIAL` — the frame's
 * material is the same for every card at every identity.
 */
export const PLATE_MATERIAL: Readonly<Record<string, string>> = Object.freeze({
  '--plate-frame-edge': source(FRAME_EDGE),
  '--plate-frame-edge-slice': slice(FRAME_EDGE),
  '--plate-art-seam': source(ART_SEAM),
  '--plate-art-seam-slice': slice(ART_SEAM),
  '--plate-header': source(HEADER_FIELD),
  '--plate-info': source(INFO_STRIP),
  '--plate-status': source(STATUS_STRIP),
  '--plate-surface-slice': slice(HEADER_FIELD),
  '--plate-pt': source(PT_PLATE),
  '--plate-pt-slice': slice(PT_PLATE),
  '--plate-identity': source(IDENTITY_WEAVE),
});

/** Round a resolved length to 2dp — enough for CSS, stable for snapshots. */
function px(value: number): string {
  return `${Math.round(value * 100) / 100}px`;
}

/**
 * The tier-dependent half: every plate's drawn band, resolved against this
 * tier's card width.
 *
 * `--plate-edge-band` is `RUNE_FRAME.ruleInset + RUNE_FRAME.rule` by
 * construction, so the frame-edge plate spans from the card's outer boundary to
 * the inner lip of the gold hairline and the card body takes over with no seam.
 * The remaining bands are the plates' own authored ratios.
 */
export function plateGeometryVars(w: number): Record<string, string> {
  const identity = PRODUCTION_FRAME_PLATES[IDENTITY_WEAVE];
  return {
    '--plate-edge-band': px(bandRatio(FRAME_EDGE) * w),
    '--plate-seam-band': px(bandRatio(ART_SEAM) * w),
    '--plate-surface-band': px(bandRatio(HEADER_FIELD) * w),
    '--plate-pt-band': px(bandRatio(PT_PLATE) * w),
    '--plate-identity-size': px((identity?.tile ?? 0.5) * w),
  };
}
