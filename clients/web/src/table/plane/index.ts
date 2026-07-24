/**
 * GameView → scene-plane staging (issue #478, Phase 1 of the 2.5D pivot under
 * ADR 0030). `stagePlane` is a **pure** function — GameView + viewport geometry
 * + ephemeral staging state → plane slot placements — the successor of
 * `buildTableScene`'s band layout, implementing
 * `docs/design/layout-model.md`:
 *
 * - **Fixed slots** that never reorder: the receiver's full-width bottom band,
 *   the focused opponent's far side, up to two wings per side staged outward
 *   from the top in stable seat order, and the clear center corridor between
 *   the far side and the receiver's band (the interaction area — nothing parks
 *   there, by construction).
 * - **The focus model as data**: exactly one focused opponent at 3+ players
 *   (none at 2); manual focus is ephemeral presentation state passed in and
 *   re-derived every view; prompt **candidates pierce every rung** — candidate
 *   objects always stage as individually addressable renders, so answering a
 *   prompt never requires a focus change.
 * - **The degradation ladder** engaged per region, independently: tier
 *   step-down → ×N folding (the carried grouping key, offered-action
 *   fingerprint included) → row wrapping inside the fixed slot → the wing
 *   digest with all-category counts; the compact change-of-kind (rung 5) is
 *   the phone-portrait branch, staging summary-tile slots.
 *
 * Geometry only: WebGL/DOM-free, no legality, every interactive rect ≥ 44 px.
 * The shipped `buildTableScene` client is untouched — this package is consumed
 * only by the fixture battlefield until the Phase 2 renderer wiring.
 */

export type {
  PlaneViewport,
  PlaneStagingState,
  PlaneRegionKind,
  WingSide,
  LadderRung,
  PlaneRender,
  WingDigest,
  PlaneRegion,
  SummaryTileSlot,
  StagedPlane,
} from './types';

export { PLANE, isPhoneGeometry, insetRect, hitRectFor } from './metrics';

export { carveSlots, carveCompactSlots } from './slots';
export type { WingSlotFrame, PlaneSlotFrames } from './slots';

export { resolveFocusSeat } from './focus';

export { buildStageItems, stageRegionContent } from './regions';
export type { StageItem, RegionContent } from './regions';

export { stagePlane } from './stage';
