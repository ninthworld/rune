import type { RenderTier } from '../../card/cardFactory';
import { TAP, TIER } from '../../tokens';
import { layout } from '../layout';
import type { SceneGeometry } from './types';

/** Layout metrics (logical px). Card sizes come from the TIER tokens. */
export const M = {
  cardGap: 10,
  rowGap: 8,
  handGap: 8,
  /** The hand fan never overlaps a card past this fraction of its width. */
  fanMaxOverlap: 0.62,
  /** A selected/lifted hand card raises by this much (the fan lift). */
  handLift: 12,
} as const;

/**
 * Default logical width used when tests build a scene without a measured shell
 * (see {@link defaultSceneGeometry}).
 */
export const DEFAULT_VIEWPORT_WIDTH = 1280;

/**
 * A geometry for callers with no measured shell (tests, fixtures): the full
 * composition carved by the real layout function at the default viewport.
 * Implemented via `layout()` so tests exercise the same carve as the live table.
 * (The import is cycle-safe: `layout.ts` imports only *types* from this module,
 * which are erased at compile time.)
 */
export function defaultSceneGeometry(
  playerCount = 2,
  viewport: { width: number; height: number } = { width: DEFAULT_VIEWPORT_WIDTH, height: 800 },
): SceneGeometry {
  return layout(viewport, playerCount).scene;
}

/**
 * A card's on-board footprint at its tier: the **rotated bounding box** when
 * tapped. Tap is ONE treatment at every tier — a ~{@link TAP.angle} rotation plus
 * a slight dim (blueprint §Card vocabulary) — so the reserved cell is the box the
 * rotated card sweeps; the row gap absorbs the swept corners.
 */
export function cellSize(tier: RenderTier, tapped: boolean): { w: number; h: number } {
  const t = TIER[tier];
  if (!tapped) return { w: t.w, h: t.h };
  return tappedFootprint(t.w, t.h);
}

/** The axis-aligned bounding box of a `w×h` card rotated by the tap angle. */
export function tappedFootprint(w: number, h: number): { w: number; h: number } {
  const c = Math.cos(TAP.angle);
  const s = Math.sin(TAP.angle);
  return { w: Math.round(w * c + h * s), h: Math.round(w * s + h * c) };
}
