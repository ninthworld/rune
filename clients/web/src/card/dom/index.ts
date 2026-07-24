/**
 * The DOM card renderer (issue #479, ADR 0030) — one component for every card
 * surface: battlefield tiers on the perspective plane, the hand fan, browsers,
 * stack rows, and inspect, all from the shared {@link CardDisplayData}
 * contract. The Pixi factory remains the shipping renderer until Phase 2; no
 * production surface consumes this package yet (the fixture battlefield, #483,
 * is its visual verification).
 */
export { CardFace } from './CardFace';
export type { CardFaceProps, CardFaceArt, CardElevation } from './CardFace';
export {
  cardFaceVars,
  faceMetrics,
  faceFootprint,
  faceAlpha,
  BATTLEFIELD_TIERS,
  PROVISIONAL,
  type CardFaceTier,
  type FaceMetrics,
} from './theme';
export { glyphStripGeometry, type GlyphStripGeometry } from './glyphStrip';
