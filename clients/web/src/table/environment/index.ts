/**
 * The battlefield environment (ADR 0030 layer 1) — issue #530, implementing
 * `docs/design/environment-system.md`.
 *
 * One environment system serves the match and the pregame places. It is a
 * noninteractive backdrop: a pure function of `(theme, viewport, quality,
 * reduced motion, failed keys)` with no state that survives a view, no hit
 * target, and no ability to gate input or block a match on an asset.
 *
 * The modules, in dependency order:
 *
 * - `zones.ts` — the §2 focal-safe geometry: zones A/B/C, the medallion
 *   sub-zone, the lip carve-out, the §6 ambient reservation, and prop placement
 *   legality. Pure geometry in canvas fractions.
 * - `planeOccupancy.ts` — re-derives the §2.1 slot union from the live
 *   `carveSlots`, so a layout change fails these tests rather than silently
 *   invalidating the art.
 * - `manifest.ts` — the §1 layer contract and the §9 per-theme manifest: the
 *   keys, load classes, byte ledger, and prop anchors the production plates of
 *   issue #548 drop into.
 * - `crop.ts` — the §4 aspect handling: one 21:9 source, centred crops, and the
 *   portrait recomposition.
 * - `quality.ts` — the §8 quality, loading, and failure matrix.
 * - `environmentScene.ts` — the `--env-*` token bridge (the `deckScene.ts` mould).
 * - `EnvironmentLayers.tsx` — the §10 layered SVG placeholder.
 * - `SceneEnvironment.tsx` — the mount both call sites use.
 */

export {
  ENV_GUARD_BAND,
  ENV_ZONES,
  ENV_MEDALLION,
  ENV_LIP_BANDS,
  ENV_AMBIENT_SPACE,
  ENV_PROP_ANCHORS,
  ENV_MID_ANCHOR_PINCH,
  right,
  bottom,
  containsRect,
  overlapsRect,
  unionRect,
  clipToCanvas,
  inFocalCore,
  inSeatFlank,
  inPropPocket,
  zoneOf,
  intersectsFocalCore,
  withinLipCarveOut,
  inAmbientSpace,
  propRect,
  propPlacementIsLegal,
  withinSeatEnvelope,
} from './zones';
export type { FractionRect, FocalZone, EnvPropAnchor, EnvPropMass } from './zones';

export {
  ENV_SEAT_COUNTS,
  ENV_REFERENCE_VIEWPORTS,
  toFractionRect,
  planeOccupancy,
  environmentBias,
} from './planeOccupancy';
export type { EnvSeatCount } from './planeOccupancy';

export {
  ENV_LAYER_IDS,
  ENV_LAYERS,
  ENV_VIEWBOX,
  ENV_AUTHORING_ASPECT,
  ENV_VARIANTS,
  ENV_MANIFESTS,
  ENV_THEME_BUDGET_BYTES,
  ENV_PLACEHOLDER_CODE_CAP_BYTES,
  propFootprint,
  ambientProps,
  themeAssetBytes,
  themeBudgetedBytes,
  ambientReservationIsQuiet,
} from './manifest';
export type {
  EnvLayerId,
  EnvLayerSpec,
  EnvVariant,
  EnvManifestKey,
  EnvLoadClass,
  EnvAssetSource,
  EnvLayerAsset,
  EnvPropEntry,
  EnvThemeManifest,
} from './manifest';

export {
  ENV_TIGHTEST_ASPECT,
  ENV_PORTRAIT_ASPECT_CEILING,
  cropForAspect,
  cropForViewport,
  containedInTightestCrop,
  spansTightestCrop,
} from './crop';
export type { EnvCrop } from './crop';

export {
  ENV_HOOKS,
  ENV_EXCURSION_PX,
  ENV_TABLET_MAX_WIDTH,
  isPortraitViewport,
  baseExcursionPx,
  ambientLevel,
  hooksFor,
  planEnvironment,
} from './quality';
export type {
  EnvViewport,
  EnvBias,
  EnvLayerTreatment,
  EnvLayerPlan,
  EnvAmbient,
  EnvironmentPlan,
  EnvHook,
  EnvironmentPlanInput,
} from './quality';

export { environmentSceneVars, environmentThemeOptions } from './environmentScene';
export type { EnvVars } from './environmentScene';

export { EnvLayerL0, EnvLayerL1, EnvLayerL2, EnvLayerL3 } from './EnvironmentLayers';
export { ENV_MEDALLION_GLYPH } from './medallion';

export { SceneEnvironment } from './SceneEnvironment';
export type { SceneEnvironmentProps } from './SceneEnvironment';
