/**
 * The DOM card renderer (issue #479, ADR 0030) — one component for every card
 * surface: battlefield tiers on the perspective plane, the hand fan, browsers,
 * stack rows, and inspect, all from the shared {@link CardDisplayData}
 * contract. The Phase 2 live match composition consumes it for the production
 * 2.5D path; the legacy Pixi table remains the parity fallback until the Phase 2
 * exit switch.
 */
export { CardFace } from './CardFace';
export type { CardFaceProps, CardFaceArt, CardElevation } from './CardFace';
export { CardArt, type CardArtMode, type CardArtProps } from './CardArt';
export { CardArtSlot, type CardArtSlotMode, type CardArtSlotProps } from './CardArt';
export {
  cardArtSlotVars,
  cardArtVars,
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
