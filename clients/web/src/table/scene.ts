/**
 * Shared pure scene-model helpers (ADR 0030), re-exported from the focused
 * scene-model modules. The legacy GameView→`TableScene` builder was retired with
 * the Pixi scene stack (#504); the live 2.5D DOM plane consumes these helpers.
 */
export {
  type Rect,
  type SurfaceTier,
  type PanelFrame,
  type SceneGeometry,
  type RenderedCard,
  type BandRowKind,
  type BandRow,
  type TargetingScene,
  type ZoneCounts,
  type Band,
  type HandRegion,
  type CombatLink,
  type AttackTarget,
  type TableScene,
  M,
  rectsOverlap,
  cellSize,
  tappedFootprint,
  localPlayerIdOf,
  orderedOpponentIds,
  bandLabel,
  zoneCountsOf,
  toDisplayData,
  hasActivatedAbilityText,
  rowKindForType,
  basicLandGlyph,
  actionFingerprint,
  groupStacks,
  actionsFor,
  declarationFor,
  tiersForSurface,
  stepDown,
} from './scene/index';
