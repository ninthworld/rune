/**
 * Shared **pure** scene-model helpers (ADR 0030). After the legacy Pixi scene
 * builder was retired (#504), this package is the shared substrate the 2.5D DOM
 * plane consumes — the card-display mapping ({@link CardDisplayData} via
 * `toDisplayData`), the ×N fold key (`groupStacks` on `cardVisualSignature`), the
 * band/zone helpers, the action subject-routing, and the geometry primitives
 * (`cellSize`, `tappedFootprint`, `rectsOverlap`) — plus the shared scene `types`.
 *
 * No game logic lives here: characteristics (P/T, counters, tapped) are passed
 * through exactly as the server computed them, and interactivity is derived solely
 * from `valid_actions[]`. The former GameView→`TableScene` builder and its
 * shell-carving `layout.ts` are gone; the live plane stages via `table/plane/`.
 */

export type {
  Rect,
  SurfaceTier,
  PanelFrame,
  SceneGeometry,
  RenderedCard,
  BandRowKind,
  BandRow,
  TargetingScene,
  ZoneCounts,
  Band,
  HandRegion,
  CombatLink,
  AttackTarget,
  TableScene,
} from './types';

export { M, rectsOverlap, cellSize, tappedFootprint } from './geometry';

export { localPlayerIdOf, orderedOpponentIds, bandLabel, zoneCountsOf } from './band-helpers';

export {
  toDisplayData,
  hasActivatedAbilityText,
  rowKindForType,
  basicLandGlyph,
  actionFingerprint,
  groupStacks,
} from './card-helpers';

export { actionsFor, declarationFor, tiersForSurface, stepDown } from './action-helpers';
