import type { RenderTier } from '../../card/cardFactory';
import { TAP, TIER } from '../../tokens';
import type { Rect } from './types';

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

/** Whether two rects overlap on a positive area (touching edges do not count).
 * The plane's fixed slots and staged regions are pairwise disjoint by
 * construction; the plane suites assert that with this shared helper (it lived on
 * the retired `layout.ts` until #504 moved it onto the surviving scene model). */
export function rectsOverlap(a: Rect, b: Rect): boolean {
  return a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h;
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
