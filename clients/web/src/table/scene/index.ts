/**
 * GameView → table scene mapping (fixed-shell anatomy, ADR 0023).
 *
 * A **pure** function that turns the store's latest {@link GameView} into the set
 * of rendered entities — one band per player panel plus the local hand — each
 * carrying the {@link CardDisplayData} the Pixi factory draws, a layout `rect`,
 * and the `valid_actions` that belong to it (ADR 0004 subject routing). Keeping
 * this pure and headless makes the whole GameView→scene mapping unit-testable
 * without a WebGL context — the React/Pixi layers only position what it returns.
 *
 * The scene lays out into the **carved panel frames** the shell layout supplies
 * ({@link SceneGeometry}, from `layout.ts`): each player's cards live inside their
 * own bounded panel, and the hand lives inside the bottom shell's hand area.
 * Fixed zone homes are what make travel animations legible and drops
 * deterministic (`docs/design/ui-blueprint.md`).
 *
 * Density ladder (blueprint §Density ladder): per panel, engaged automatically —
 * full tier for the surface, then one card-tier step down, then aggressive ×N
 * folding (the stack grouping below), then vertical compression as the last
 * resort. Each panel picks its own rung: one hoarding opponent never shrinks the
 * others.
 *
 * No game logic lives here: characteristics (P/T, counters, tapped) are passed
 * through exactly as the server computed them, and interactivity is derived
 * solely from `valid_actions[]`.
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

export {
  M,
  DEFAULT_VIEWPORT_WIDTH,
  defaultSceneGeometry,
  cellSize,
  tappedFootprint,
} from './geometry';

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

export { flowRow, layPanel, layHand } from './row-layout';

export { buildTableScene } from './builder';
