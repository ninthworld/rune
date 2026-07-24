import type { Rect } from '../scene/types';
import type { PlaneViewport } from './types';

/**
 * Plane staging metrics (logical px, or fractions of the plane dimension noted).
 * The proportions carry the staging prototype's evidence
 * (`prototypes/ui-2-5d-layouts-v1.html`, reference-only) into the model of
 * `docs/design/layout-model.md`; the 44 px floor is normative
 * (`docs/design/presentation-budgets.md` §Accessibility).
 */
export const PLANE = {
  /** Inset from a slot rect to its card content area. */
  pad: 10,
  /** Vertical gap between type-grouped rows. */
  rowGap: 8,
  /** Horizontal gap between cards in a row. */
  cardGap: 6,
  /** The interactive-target floor: every hotspot is at least this square. */
  minHit: 44,
  /** Crest cluster footprint (≥ minHit — the player-targeting surface). */
  crest: { w: 52, h: 52 },
  /** Zone-pile cluster footprint. */
  pile: { w: 44, h: 62 },
  /** The receiver's band: full-width bottom third (±), fractions of W/H. */
  receiver: { x: 0.12, w: 0.76, h: 0.33 },
  /** The far side at 2 players: the opponent, full width. */
  duelFar: { x: 0.12, y: 0.02, w: 0.76, h: 0.34 },
  /** The far side at 3+ players: the focused opponent, top center. */
  far: { x: 0.2, y: 0.02, w: 0.6, h: 0.34 },
  /** Wing staging: outward from the top, up to two per side. */
  wing: {
    /** First wing rank's top, as a fraction of H. */
    top: 0.12,
    /** Fraction of a wing's width staged past the plane edge (the felt bleed)
     * — what keeps wing inner edges clear of the center corridor. */
    bleed: 0.28,
    /** One wing per side (3–4 players): the larger wing. */
    single: { w: 0.24, h: 0.4 },
    /** Two wings per side (5–6 players): the digest-rung wing. */
    double: { w: 0.21, h: 0.25 },
    /** Vertical gap between wing ranks, as a fraction of H. */
    rankGap: 0.03,
  },
  /** Compact change-of-kind staging (rung 5, phone portrait, 3+ players). */
  compact: {
    /** The receiver's band height, as a fraction of H. */
    receiverH: 0.4,
    /** The focused opponent's drawn board. */
    far: { x: 0.06, y: 8, w: 0.88, h: 0.3 },
    /** Summary tiles: ≥ minHit tall, stacked below the focused board. */
    tile: { x: 0.06, w: 0.5, h: 48, gap: 8, topGap: 12, stripGap: 8 },
  },
} as const;

/**
 * Whether the viewport takes the phone-portrait staging branch (the compact
 * change-of-kind at 3+ players): portrait orientation, per layout-model rung 5.
 */
export function isPhoneGeometry(viewport: PlaneViewport): boolean {
  return viewport.height > viewport.width;
}

/** A rect inset by `by` on every side (clamped to non-negative dimensions). */
export function insetRect(rect: Rect, by: number): Rect {
  return {
    x: rect.x + by,
    y: rect.y + by,
    w: Math.max(0, rect.w - 2 * by),
    h: Math.max(0, rect.h - 2 * by),
  };
}

/**
 * The interactive hotspot for a drawn rect: the rect grown (centered) to the
 * 44 px floor in any dimension that falls short. The drawn footprint never
 * shrinks — only the hit target grows.
 */
export function hitRectFor(rect: Rect, min: number = PLANE.minHit): Rect {
  const w = Math.max(rect.w, min);
  const h = Math.max(rect.h, min);
  return {
    x: rect.x - Math.floor((w - rect.w) / 2),
    y: rect.y - Math.floor((h - rect.h) / 2),
    w,
    h,
  };
}
