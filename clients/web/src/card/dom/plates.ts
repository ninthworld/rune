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

/** A plate's band as a fraction of the card width `W`. */
export function bandRatio(key: string): number {
  return PRODUCTION_FRAME_PLATES[key]?.band ?? 0;
}

/**
 * One plate as a complete `border-image` shorthand:
 * `<source> <slice> [fill] / <width> [/ <outset>] stretch`.
 *
 * The band is written as `calc(var(--face-w) * ratio)` rather than resolved to
 * px here, which is the whole reason this can be a **constant**. `--face-w` is
 * already published per tier, so one frozen string serves every tier and the
 * browser does the arithmetic — and the face's style attribute, which the plane
 * reconciler rewrites on every view, grows by one property per surface instead
 * of a source, a slice, and a band.
 *
 * That mattered more than it looks: publishing them separately added sixteen
 * custom properties to every card and cost ~29% of the reconnect rebuild
 * budget on a 120-permanent board, which CI caught at the boundary.
 *
 * A key that did not ship resolves to `none`, a valid `border-image` that draws
 * nothing — an inert declaration rather than an invalid one that would drop the
 * whole rule.
 */
function borderImage(key: string, { fill = false, outset = '' } = {}): string {
  const plate = PRODUCTION_FRAME_PLATES[key];
  if (plate === undefined) return 'none';
  const band = `calc(var(--face-w) * ${plate.band})`;
  const tail = outset === '' ? band : `${band} / ${outset}`;
  return `url(${plate.src}) ${plate.slice}${fill ? ' fill' : ''} / ${tail} stretch`;
}

/**
 * The identity material as a complete background **layer**, tile size included,
 * for the same reason: one constant instead of an image plus a size.
 */
function backgroundLayer(key: string): string {
  const plate = PRODUCTION_FRAME_PLATES[key];
  if (plate === undefined) return 'none';
  const size = `calc(var(--face-w) * ${plate.tile ?? 0.5})`;
  return `url(${plate.src}) 0 0 / ${size} ${size} repeat`;
}

/**
 * Which plate each surface draws, complete. Frozen once, exactly like
 * `theme.ts`'s `MATERIAL` — the frame's material is the same for every card at
 * every identity, and nothing here varies by tier.
 *
 * The frame edge is the one entry with an outset: it is pushed back out to the
 * card's own boundary by exactly the hairline inset, so its band —
 * `ruleInset + rule` by construction — spans from the card edge to the inner
 * lip of the hairline and the card body takes over with no seam.
 */
export const PLATE_MATERIAL: Readonly<Record<string, string>> = Object.freeze({
  '--plate-frame-edge': borderImage(FRAME_EDGE, { outset: 'var(--rule-inset)' }),
  '--plate-art-seam': borderImage(ART_SEAM),
  '--plate-header': borderImage(HEADER_FIELD, { fill: true }),
  '--plate-info': borderImage(INFO_STRIP, { fill: true }),
  '--plate-status': borderImage(STATUS_STRIP, { fill: true }),
  '--plate-pt': borderImage(PT_PLATE, { fill: true }),
  '--plate-identity': backgroundLayer(IDENTITY_WEAVE),
});
