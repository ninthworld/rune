/**
 * The passive WebGL effects layer (issue #482, ADR 0030 layer 3) — Pixi's
 * refocused role under the 2.5D architecture: paths, bursts, glows — never
 * cards, never a hit target. Render-on-demand with zero idle cost by
 * construction (the spike's decisive finding), a data-driven v1 vocabulary
 * from the asset-pipeline taxonomy (categories + source/target anchors +
 * accent — never bespoke per card), endpoint tracking during reconciler
 * motion, and the quality-level particle caps with the independent
 * effect-density control.
 */
export {
  PARTICLE_CAP,
  DENSITY_SCALE,
  type EffectQuality,
  type EffectDensity,
  type EffectAnchor,
  type TransientCategory,
  type TransientInvocation,
  type PersistentCategory,
  type PersistentEffect,
  type DrawOp,
} from './types';

export {
  rectCenter,
  anchorCenter,
  pathCurve,
  dashSegments,
  arrowHead,
  burstParticles,
  type Point,
  type BurstParticle,
} from './geometry';

export {
  EffectsLayer,
  EFFECT_TIMING,
  drawProgram,
  createEffectsTicker,
  type EffectsLayerOptions,
} from './layer';
