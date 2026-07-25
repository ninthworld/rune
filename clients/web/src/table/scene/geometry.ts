import type { RenderTier } from '../../card/cardFactory';
import {
  faceFootprint,
  faceMetrics,
  surfaceKindFor,
  type CardFaceTier,
  type CardSurfaceKind,
} from '../../card/dom';
import { SPLAY, TAP } from '../../tokens';
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
 *
 * The box comes from `card/dom`'s {@link faceFootprint} and nothing else
 * (card-representation §3.1/§4, issue #529): a square permanent and a wide land
 * **resource tile** at the same tier have different boxes, so the silhouette has
 * to be passed in — the cell can never be keyed on the tier alone. `kind` is
 * omitted only where the caller genuinely has no row (it then resolves to the
 * tier's default silhouette).
 */
export function cellSize(
  tier: RenderTier,
  tapped: boolean,
  kind?: CardSurfaceKind,
): { w: number; h: number } {
  return faceFootprint(tier as CardFaceTier, tapped, kind);
}

/**
 * The silhouette a staged permanent draws at `tier`: the land **resource tile**
 * for a permanent the staging layer sorted into the lands row, the square plaque
 * otherwise (card-representation §3.1/§4). The same server-type-line display
 * glue that picks `landGlyph` — never an inference by the renderer.
 */
export function surfaceKindForRow(tier: RenderTier, landRow: boolean): CardSurfaceKind {
  return surfaceKindFor(tier as CardFaceTier, landRow);
}

/**
 * Vertical clearance a cell needs **above** its box for the `×N` count tab
 * (card-representation §7.4, issue #529): the tab is a top-edge plate centred on
 * the card's top edge and overhanging it by half its own height. A row that does
 * not reserve this would let a fold's count collide with the row above.
 */
export function tabClearance(tier: RenderTier, kind?: CardSurfaceKind): number {
  const m = faceMetrics(tier as CardFaceTier, kind);
  return Math.ceil((m.tab * 1.35) / 2);
}

/**
 * The overhang a **folded ×N pile** sweeps outside its own box (issue #529): the
 * splay steps **down-and-left** by (`SPLAY.stepX`·W, `SPLAY.stepY`·H) per hidden
 * layer, capped at {@link SPLAY.maxLayers}, plus the accent edge. Row and slot
 * padding must clear it, or a pile's depth would underlap its left neighbour.
 */
export function splayClearance(
  tier: RenderTier,
  kind?: CardSurfaceKind,
): { left: number; down: number } {
  const m = faceMetrics(tier as CardFaceTier, kind);
  return {
    left: Math.ceil(SPLAY.maxLayers * SPLAY.stepX * m.w) + SPLAY.edgePx,
    down: Math.ceil(SPLAY.maxLayers * SPLAY.stepY * m.h) + SPLAY.edgePx,
  };
}

/** The axis-aligned bounding box of a `w×h` card rotated by the tap angle. */
export function tappedFootprint(w: number, h: number): { w: number; h: number } {
  const c = Math.cos(TAP.angle);
  const s = Math.sin(TAP.angle);
  return { w: Math.round(w * c + h * s), h: Math.round(w * s + h * c) };
}
