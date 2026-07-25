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
 * The shipped `buildTableScene` client remains the parity path through Phase 2;
 * this package is consumed by both the fixture battlefield and the opt-in live
 * match composition.
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

export {
  PLANE,
  isCompactGeometry,
  insetRect,
  hitRectFor,
  withinEnvelope,
  clampToEnvelope,
} from './metrics';

export { stageSeatCluster, clusterD, fitName, damageEscalation } from './cluster';
export type {
  ClusterVariant,
  PlateDirection,
  LifeUrgency,
  DamageEscalation,
  ClusterChipKind,
  ClusterChip,
  ClusterPlate,
  ClusterPip,
  ClusterChannels,
  SeatClusterFacts,
  SeatClusterRequest,
  SeatCluster,
} from './cluster';

export { stageSeatHandFan, HAND_FAN } from './seatHandFan';
export type { HandFanSlot, SeatHandFan, SeatHandFanRequest } from './seatHandFan';

export { stageRack, digestExpansionRects, RACK_ZONES } from './rack';
export type { RackZone, RackVariant, RackSlot, SeatRack, RackRequest } from './rack';

export { carveSlots, carveCompactSlots } from './slots';
export type { WingSlotFrame, PlaneSlotFrames } from './slots';

export { resolveFocusSeat } from './focus';

export { buildStageItems, stageRegionContent } from './regions';
export type { StageItem, RegionContent } from './regions';

export { stagePlane } from './stage';
