/**
 * Focal-safe geometry for the battlefield environment
 * (`docs/design/environment-system.md` §2, §4, §6) — issue #530.
 *
 * Every rect here is expressed in **fractions of the composed canvas** (the
 * viewport the scene plane fills), never of the source plate, so one set of
 * numbers governs the SVG placeholder and the raster plates that replace it
 * (§10.2). Nothing in this module reads the DOM, the view, or the art: it is
 * pure geometry, which is exactly why the §2.5 assertions can be unit tests
 * rather than a browser check.
 *
 * The derivation is not invented here. §2.1 overlays every slot rect the plane
 * can occupy at 2–6 seats (`table/plane/metrics.ts`), clips to the canvas, takes
 * the union, and adds a 2 % guard band for hand-fan overhang and contact
 * shadows. The result is the three zones below; `planeOccupancy.ts` re-derives
 * the union from the live `carveSlots` so a layout change fails this module's
 * tests instead of silently invalidating the art.
 */

/** A rect in fractions of the composed canvas: `0 ≤ x, y` and `x + w ≤ 1`. */
export interface FractionRect {
  /** Left edge, fraction of canvas width. */
  x: number;
  /** Top edge, fraction of canvas height. */
  y: number;
  /** Width, fraction of canvas width. */
  w: number;
  /** Height, fraction of canvas height. */
  h: number;
}

/**
 * Which focal-safe zone a piece of illustrated incident may live in (§2.2):
 *
 * - `A` — the focal core. L1 only, plus the §2.4 lip carve-out. Cards sit here.
 * - `B` — the seat flanks. L2, and L3 only as low-mass ground cover.
 * - `C` — the four prop pockets. The full L3 vocabulary; 8.6 % of canvas area.
 */
export type FocalZone = 'A' | 'B' | 'C';

/** The guard band §2.1 adds to the derived slot union before zoning. */
export const ENV_GUARD_BAND = 0.02;

/**
 * The normative zones of §2.2, byte-identical across every theme (§5.1). The
 * one-line answer the document states: **the focal-safe rectangle is
 * `x 10 %–90 %, y 0 %–100 %` — the central 80 % of width at full height.**
 */
export const ENV_ZONES = {
  /** Zone A — the focal core. No L2 and no L3 ever, but for the §2.4 lips. */
  focalCore: { x: 0.1, y: 0, w: 0.8, h: 1 },
  /** Zone B — the seat flanks: covered at 3–6 seats, revealed at 2. */
  seatFlanks: [
    { x: 0, y: 0.1, w: 0.1, h: 0.57 },
    { x: 0.9, y: 0.1, w: 0.1, h: 0.57 },
  ],
  /** Zone C — the four prop pockets, where the theme's personality lives. */
  propPockets: [
    { x: 0, y: 0, w: 0.1, h: 0.1 },
    { x: 0.9, y: 0, w: 0.1, h: 0.1 },
    { x: 0, y: 0.67, w: 0.1, h: 0.33 },
    { x: 0.9, y: 0.67, w: 0.1, h: 0.33 },
  ],
} as const satisfies { focalCore: FractionRect; [k: string]: unknown };

/**
 * The medallion sub-zone (§2.3): the only permitted L1 incident inside Zone A,
 * transcribed from the baseline. `r` is a fraction of canvas **width** on both
 * axes — the mark is a circle in canvas space, not an ellipse — and the centre
 * is the anchor the tightest crop must preserve (§4.3) and the point phone
 * portrait pins the recomposition to (§4.5).
 */
export const ENV_MEDALLION = { cx: 0.5, cy: 0.4, r: 0.05 } as const;

/**
 * The §2.4 lip carve-out: the only way L2 may cross Zone A. Two broad
 * horizontal bands, no silhouette taller than 8 % of H, no vertical element.
 * L3 has no carve-out and never enters Zone A at any height.
 */
export const ENV_LIP_BANDS: readonly FractionRect[] = [
  { x: 0, y: 0, w: 1, h: 0.08 },
  { x: 0, y: 0.92, w: 1, h: 0.08 },
];

/**
 * The `AMBIENT SPACE — FUTURE` reservation of §6, measured off panels 6–8:
 * `x ∈ [0 %, 20 %], y ∈ [69 %, 97 %]`, with a chamfer taking the inboard edge to
 * `x = 12 %` above `y = 80 %`. Compositionally reserved in every theme; at most
 * one L3 prop may be anchored inside it (§6.2), and nothing carrying rules
 * information may ever occupy it (§6.1).
 */
export const ENV_AMBIENT_SPACE = {
  rect: { x: 0, y: 0.69, w: 0.2, h: 0.28 } as FractionRect,
  /** The chamfer: above this `y`, the inboard edge pulls back to `chamferX`. */
  chamferY: 0.8,
  /** The pulled-back inboard edge above {@link chamferY}. */
  chamferX: 0.12,
  /** The subregion no seat contests at any count (§6.3) — the bottom-left pocket. */
  uncontested: { x: 0, y: 0.69, w: 0.1, h: 0.31 } as FractionRect,
} as const;

/** The six prop anchor names of §4.4. A theme may leave one empty, never add one. */
export const ENV_PROP_ANCHORS = [
  'top-left',
  'top-right',
  'bottom-left',
  'bottom-right',
  'left-mid',
  'right-mid',
] as const;

/** One of the six anchors a prop may hang from. */
export type EnvPropAnchor = (typeof ENV_PROP_ANCHORS)[number];

/**
 * The mass class of an L3 prop. `low` ground cover (verge, water, small
 * foliage) may sit in Zone B; anything `full` — a tall silhouette or a light
 * source — is confined to Zone C.
 */
export type EnvPropMass = 'low' | 'full';

/** The `x` pinch §4.4 confines the mid anchors to. */
export const ENV_MID_ANCHOR_PINCH = 0.04;

// ── Rect algebra ─────────────────────────────────────────────────────────────

/** The right edge of a rect. */
export function right(r: FractionRect): number {
  return r.x + r.w;
}

/** The bottom edge of a rect. */
export function bottom(r: FractionRect): number {
  return r.y + r.h;
}

/** Whether `inner` lies entirely inside `outer` (tolerant of float noise). */
export function containsRect(outer: FractionRect, inner: FractionRect, epsilon = 1e-9): boolean {
  return (
    inner.x >= outer.x - epsilon &&
    inner.y >= outer.y - epsilon &&
    right(inner) <= right(outer) + epsilon &&
    bottom(inner) <= bottom(outer) + epsilon
  );
}

/** Whether two rects share any area (touching edges do not count). */
export function overlapsRect(a: FractionRect, b: FractionRect, epsilon = 1e-9): boolean {
  return (
    a.x < right(b) - epsilon &&
    b.x < right(a) - epsilon &&
    a.y < bottom(b) - epsilon &&
    b.y < bottom(a) - epsilon
  );
}

/** The smallest rect containing every input; `undefined` for an empty list. */
export function unionRect(rects: readonly FractionRect[]): FractionRect | undefined {
  if (rects.length === 0) return undefined;
  let x0 = Infinity;
  let y0 = Infinity;
  let x1 = -Infinity;
  let y1 = -Infinity;
  for (const r of rects) {
    x0 = Math.min(x0, r.x);
    y0 = Math.min(y0, r.y);
    x1 = Math.max(x1, right(r));
    y1 = Math.max(y1, bottom(r));
  }
  return { x: x0, y: y0, w: x1 - x0, h: y1 - y0 };
}

/** A rect clipped to the canvas — the clip §2.1 applies before the union. */
export function clipToCanvas(r: FractionRect): FractionRect {
  const x0 = Math.max(0, Math.min(1, r.x));
  const y0 = Math.max(0, Math.min(1, r.y));
  const x1 = Math.max(0, Math.min(1, right(r)));
  const y1 = Math.max(0, Math.min(1, bottom(r)));
  return { x: x0, y: y0, w: Math.max(0, x1 - x0), h: Math.max(0, y1 - y0) };
}

// ── Zone membership ──────────────────────────────────────────────────────────

/** Whether a rect lies entirely inside Zone A (the focal core). */
export function inFocalCore(r: FractionRect): boolean {
  return containsRect(ENV_ZONES.focalCore, r);
}

/** Whether a rect lies entirely inside one seat flank (Zone B). */
export function inSeatFlank(r: FractionRect): boolean {
  return ENV_ZONES.seatFlanks.some((flank) => containsRect(flank, r));
}

/** Whether a rect lies entirely inside one prop pocket (Zone C). */
export function inPropPocket(r: FractionRect): boolean {
  return ENV_ZONES.propPockets.some((pocket) => containsRect(pocket, r));
}

/**
 * The single zone a rect lies wholly within, or `undefined` when it straddles
 * two. Zone C is checked before Zone B: the pockets and the flanks abut at
 * `y = 10 %` and `y = 67 %` but never overlap, so the order only fixes the
 * answer for a degenerate zero-height rect on the boundary.
 */
export function zoneOf(r: FractionRect): FocalZone | undefined {
  if (inPropPocket(r)) return 'C';
  if (inSeatFlank(r)) return 'B';
  if (inFocalCore(r)) return 'A';
  return undefined;
}

/**
 * Whether a rect touches the focal core at all — the §2.2 prohibition L2 and L3
 * are held to. An anchor tagged `lip` is exempt via {@link withinLipCarveOut}.
 */
export function intersectsFocalCore(r: FractionRect): boolean {
  return overlapsRect(ENV_ZONES.focalCore, r);
}

/**
 * Whether a rect crossing Zone A is legal under the §2.4 lip carve-out: it must
 * lie wholly inside one of the two bands, which caps its silhouette at 8 % of H
 * by construction.
 */
export function withinLipCarveOut(r: FractionRect): boolean {
  return ENV_LIP_BANDS.some((band) => containsRect(band, r));
}

/** Whether a rect lies inside the §6 ambient-space reservation, chamfer included. */
export function inAmbientSpace(r: FractionRect): boolean {
  if (!containsRect(ENV_AMBIENT_SPACE.rect, r)) return false;
  // Above the chamfer line the inboard edge pulls back; a rect that reaches
  // into the chamfered corner is outside the reservation.
  if (r.y < ENV_AMBIENT_SPACE.chamferY) return true;
  return right(r) <= ENV_AMBIENT_SPACE.chamferX + 1e-9;
}

/**
 * The footprint a prop occupies on the composed canvas: its anchor corner,
 * offset inboard by `offset`, sized by `size`. Offsets and sizes are fractions
 * of the canvas (§4.4), so a prop sits the same distance from its corner at
 * 16:9 and at 21:9 — the mechanism that makes one source set serve both without
 * a second composition.
 */
export function propRect(
  anchor: EnvPropAnchor,
  offset: { x: number; y: number },
  size: { w: number; h: number },
): FractionRect {
  const fromRight = anchor === 'top-right' || anchor === 'bottom-right' || anchor === 'right-mid';
  const fromBottom = anchor === 'bottom-left' || anchor === 'bottom-right';
  const mid = anchor === 'left-mid' || anchor === 'right-mid';
  const x = fromRight ? 1 - offset.x - size.w : offset.x;
  const y = mid ? offset.y : fromBottom ? 1 - offset.y - size.h : offset.y;
  return { x, y, w: size.w, h: size.h };
}

/**
 * Whether a prop's placement satisfies §2.2 and §4.4: `full` mass belongs to
 * Zone C, `low` mass may also sit in Zone B, and the two mid anchors are limited
 * to `low` mass inside the `x < 4 %` / `x > 96 %` pinch the baseline's crystal
 * plinths occupy.
 */
export function propPlacementIsLegal(
  anchor: EnvPropAnchor,
  rect: FractionRect,
  mass: EnvPropMass,
): boolean {
  if (intersectsFocalCore(rect)) return false;
  const mid = anchor === 'left-mid' || anchor === 'right-mid';
  if (mid) {
    if (mass !== 'low') return false;
    const pinched =
      anchor === 'left-mid'
        ? right(rect) <= ENV_MID_ANCHOR_PINCH + 1e-9
        : rect.x >= 1 - ENV_MID_ANCHOR_PINCH - 1e-9;
    if (!pinched) return false;
  }
  const zone = zoneOf(rect);
  if (zone === 'C') return true;
  return zone === 'B' && mass === 'low';
}

/**
 * Whether a rect lies inside the **seat envelope** — Zone A ∪ Zone B, the
 * region §3.3 lets layout move a seat anywhere within without consulting the
 * art. This is the containment §2.5 assertion 3 checks for every seat count.
 *
 * The union is not a rectangle, so the test is per x-band: whatever part of the
 * rect falls in a flank must also fall inside that flank's `y` range, while the
 * part inside the core is unconstrained vertically.
 */
export function withinSeatEnvelope(r: FractionRect, epsilon = 1e-9): boolean {
  const core = ENV_ZONES.focalCore;
  const flank = ENV_ZONES.seatFlanks[0]!;
  const touchesLeftFlank = r.x < core.x - epsilon;
  const touchesRightFlank = right(r) > right(core) + epsilon;
  if (!touchesLeftFlank && !touchesRightFlank) return true;
  // Any part outside the core must sit inside the flanks' shared `y` band.
  return r.y >= flank.y - epsilon && bottom(r) <= bottom(flank) + epsilon;
}
