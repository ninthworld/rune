/**
 * The **hand fan curve family** (issue #533) — one curve model, one overlap
 * rule, one paging mechanism, shared by every hand on the table.
 *
 * There are two hands in RUNE and they live on different surfaces: the
 * receiver's own hand is a screen-space shell region (ADR 0032 §7 — "the hand
 * remains a shell region, not a scene-drawn object"), while every opponent's is
 * a face-down fan staged on the plane beside that seat's identity cluster. The
 * baselines draw them as **one family**: the same broad arc, the same "outer
 * cards angle more than centre cards" rotation, the same overlap. So the
 * geometry lives here once and both surfaces read it at different
 * {@link FanTier}s. Writing the fan twice is how the two end up with subtly
 * different curvature; this module exists so that cannot happen.
 *
 * ## The curve
 *
 * A fan of `n` cards places card `i` at three derived values, all pure
 * functions of `(i, n)` and the tier:
 *
 * - {@link fanFraction} — `i / (n - 1)`, the position along the fan's own span
 *   (`0.5` for a single card). This is #528's endpoint rule, unchanged: the
 *   consuming surface insets that span by half a card so an endpoint can never
 *   be clipped (`shellLayout.ts` invariant I2).
 * - {@link fanAngle} — a linear ramp from `0` at the centre to the fan's
 *   rotation cap at each end. The cap itself grows with the hand
 *   (`stepDeg` per card away from centre) until it saturates at `maxDeg`, so a
 *   two-card hand is nearly flat and a twelve-card hand is a full arc.
 * - {@link fanDip} — a parabolic drop, so the outer cards sit lower than the
 *   centre ones and the row reads as an arc rather than a rotated strip.
 *
 * ## Compression, then paging
 *
 * `layout-model.md` §Stress dispositions fixes the order: *"the fan compresses
 * spacing and rotation before card size; when exposed spacing would drop below
 * the 44 px floor, the fan pages (page size derived from the floor, ≥ 44 px
 * page controls, board stays visible)"*. {@link fanPlan} is that rule:
 *
 * 1. The count picks a band ({@link handCountBand}: 0–7, 8–12, 13–20, 20+) and
 *    the band picks a **target exposure** — the widest gap the fan will use.
 *    A small hand therefore stays a tight fan in the middle of its span rather
 *    than being stretched to the span's edges.
 * 2. The available span is spent down to that target; past it the fan keeps
 *    compressing toward the tier's **exposure floor**.
 * 3. The floor is never crossed. The page size is derived from it, so
 *    `exposure ≥ tier.minExposure` holds for every page of every plan — that
 *    is the 44 px guarantee for the local hand, and the "a back is still
 *    legible as a card" guarantee for an opponent's.
 *
 * Card size is never traded away: every tier renders at its own fixed box, so
 * compression only ever moves cards closer together and rotates them further.
 *
 * ## Hidden information
 *
 * Nothing in this module accepts a card, an entity id, a zone, or a view. A
 * fan slot is a function of `(index, count, tier)` and nothing else, which is
 * what makes an opponent's fan structurally incapable of expressing anything
 * but its count (`card-representation.md` §13.1, and the issue's added
 * acceptance criterion). `plane/seatHandFan.ts` and `handFan.test.ts` assert
 * that shape rather than leaving it to review.
 */
import { TIER } from '../tokens';
import { PLANE } from './plane/metrics';

/**
 * The count bands issue #533 requires the fan to define: an opening hand, a
 * wide hand, a deep hand, and the overflow case beyond twenty cards. The band
 * selects the fan's target exposure ({@link FAN.target}) and is published to
 * the DOM so the treatment can differ per band without a second geometry path.
 */
export type HandCountBand = 'opening' | 'wide' | 'deep' | 'overflow';

/** Which band a hand of `count` cards falls in (0–7, 8–12, 13–20, 20+). */
export function handCountBand(count: number): HandCountBand {
  if (count <= 7) return 'opening';
  if (count <= 12) return 'wide';
  return count <= 20 ? 'deep' : 'overflow';
}

/**
 * The curve constants shared by every fan. Proportions, not pixels: each is a
 * fraction of the tier's own card box, so the two tiers below are the *same*
 * fan at two scales rather than two tunings of two fans.
 */
export const FAN = {
  /**
   * Rotation added per card away from the fan's centre, in degrees, before the
   * cap. What makes a two-card hand nearly flat and a large one a full arc.
   */
  stepDeg: 2.4,
  /**
   * The widest gap the fan will open between neighbouring cards, per count
   * band, as a fraction of the card's width. The opening band is the baseline's
   * broad, legible fan; each further band closes the overlap before the fan
   * has to page. `overflow` is `0`, meaning "use the tier's floor".
   */
  target: { opening: 0.68, wide: 0.5, deep: 0.38, overflow: 0 },
  /** Reserved on each side of the band for a page control, when the fan pages. */
  pagerW: 48,
} as const;

/**
 * One fan tier: the card box it draws at plus the three numbers that shape its
 * arc. The **only** thing that differs between the receiver's hand and an
 * opponent's is this record.
 */
export interface FanTier {
  /** The card footprint, in px. Never traded away by compression. */
  card: { w: number; h: number };
  /** Cap on the outermost card's rotation, in degrees. */
  maxDeg: number;
  /** Arc depth at the fan's ends, as a fraction of the card's height. */
  arcFrac: number;
  /**
   * The smallest distance between neighbouring card origins the fan will ever
   * use. For an interactive fan this is the 44 px accessibility floor; for a
   * decorative one it is the width below which a back stops reading as a card.
   */
  minExposure: number;
  /** Clearance kept between the outermost card's edge and the span's edge. */
  gutter: number;
}

/**
 * The receiver's own hand: the `hand` card tier of `card-representation.md`
 * §8.1, and the 44 px interactive floor of `presentation-budgets.md`
 * §Accessibility. Every card here is a hit target, so the floor is normative.
 */
export const LOCAL_FAN_TIER: FanTier = {
  card: { w: TIER.hand.w, h: TIER.hand.h },
  maxDeg: 9,
  arcFrac: 0.075,
  minExposure: PLANE.minHit,
  gutter: 4,
};

/**
 * An opponent's face-down fan, at a seat cluster whose scale unit is `d`
 * (`seat-identity.md` §1.1). The card box is the `hand` tier's **aspect** at
 * the cluster's scale — one silhouette across hand, library, travel, and piles
 * (`card-representation.md` §13) — and nothing in the fan is interactive, so
 * the floor is a legibility floor rather than the 44 px touch floor: a back
 * still has to read as a separate card.
 */
export function opponentFanTier(d: number): FanTier {
  const w = Math.round(d * 0.42);
  return {
    card: { w, h: Math.round((w * TIER.hand.h) / TIER.hand.w) },
    maxDeg: 9,
    arcFrac: 0.075,
    minExposure: Math.max(4, Math.round(w * 0.34)),
    gutter: 2,
  };
}

/** The span an opponent fan is laid out across, for a cluster scale unit `d`. */
export function opponentFanSpan(d: number): number {
  return Math.round(d * 2.6);
}

/**
 * A card's position along the fan's own span, as a fraction in `[0, 1]`.
 * A single card centres. Identical to `shellLayout.handFanFraction`, which the
 * shell keeps as its published name.
 */
export function fanFraction(index: number, count: number): number {
  if (count <= 1) return 0.5;
  return index / (count - 1);
}

/** The signed distance from the fan's centre, normalised to `[-1, 1]`. */
function centreAxis(index: number, count: number): number {
  if (count <= 1) return 0;
  const half = (count - 1) / 2;
  return (index - half) / half;
}

/**
 * The rotation of the card at `index`, in degrees: `0` at the fan's centre,
 * ramping linearly to the fan's cap at each end. The cap is
 * `min(tier.maxDeg, ((count - 1) / 2) · FAN.stepDeg)`, so rotation grows with
 * the hand until it saturates — outer cards always angle more than centre ones.
 */
export function fanAngle(index: number, count: number, tier: FanTier): number {
  if (count <= 1) return 0;
  const cap = Math.min(tier.maxDeg, ((count - 1) / 2) * FAN.stepDeg);
  return centreAxis(index, count) * cap;
}

/**
 * How far below the fan's centre line the card at `index` sits, in px — the
 * parabolic drop that makes the row read as an arc. `0` at the centre,
 * `tier.arcFrac · cardHeight` at each end.
 */
export function fanDip(index: number, count: number, tier: FanTier): number {
  if (count <= 1) return 0;
  const u = centreAxis(index, count);
  return tier.arcFrac * tier.card.h * u * u;
}

/** The span a fan can actually spread origins across: minus one card and both gutters. */
export function fanUsableSpan(span: number, tier: FanTier): number {
  return Math.max(0, span - tier.card.w - 2 * tier.gutter);
}

/**
 * How many cards fit in `span` while every one of them keeps
 * `tier.minExposure` of itself exposed. This is the number the page size is
 * derived from, which is what makes the floor a guarantee rather than a hope.
 */
export function fanCapacity(span: number, tier: FanTier): number {
  const usable = fanUsableSpan(span, tier);
  if (usable <= 0) return 1;
  return 1 + Math.floor(usable / tier.minExposure);
}

/** A resolved fan: how it pages, how tightly it packs, and how wide it sits. */
export interface FanPlan {
  /** Total cards in the hand. */
  count: number;
  /** The count band the hand fell in. */
  band: HandCountBand;
  /** The span the fan was laid out in, in px. */
  span: number;
  /** Cards on a full page — `count` when the whole hand fits. */
  pageSize: number;
  /** How many pages the hand takes; `0` for an empty hand, else `≥ 1`. */
  pages: number;
  /** Distance between neighbouring card origins, in px. Never below the floor. */
  exposure: number;
  /** The fan's drawn width: `exposure · (pageSize - 1) + card width`. */
  width: number;
  /** Half the span the fan does not use — what centres it (see {@link FanPlan.width}). */
  slack: number;
}

/**
 * Resolve a hand of `count` cards into a span. Compression first, paging only
 * at the floor: see the module header.
 *
 * `pages > 1` means the surface must offer page controls; the caller reserves
 * their width and calls again (the shell's {@link localFanPlan} does exactly
 * that), because reserving space unconditionally would push an opening hand on
 * phone portrait into paging it does not need.
 */
export function fanPlan(count: number, span: number, tier: FanTier): FanPlan {
  const band = handCountBand(count);
  const usable = fanUsableSpan(span, tier);
  const capacity = fanCapacity(span, tier);
  const pageSize = Math.max(1, Math.min(count, capacity));
  const pages = count <= 0 ? 0 : Math.ceil(count / pageSize);
  const targetFrac = FAN.target[band];
  const target = targetFrac === 0 ? tier.minExposure : targetFrac * tier.card.w;
  // Spread across the page's share of the span, but never wider than the band's
  // target and never tighter than the floor. `pageSize - 1 ≤ capacity - 1 =
  // ⌊usable / minExposure⌋`, so the spread term is itself ≥ the floor: the
  // clamp below can only ever be the *upper* bound biting.
  const spread = pageSize <= 1 ? 0 : usable / (pageSize - 1);
  const exposure =
    pageSize <= 1 ? 0 : Math.max(tier.minExposure, Math.min(spread, Math.max(target, 0)));
  const width = exposure * Math.max(0, pageSize - 1) + tier.card.w;
  return {
    count,
    band,
    span,
    pageSize,
    pages,
    exposure: pageSize <= 1 ? tier.card.w : exposure,
    width,
    slack: Math.max(0, (usable - exposure * Math.max(0, pageSize - 1)) / 2),
  };
}

/** The half-card (plus gutter, plus centring slack) a fan's span is inset by. */
export function fanInset(plan: FanPlan, tier: FanTier): number {
  return tier.card.w / 2 + tier.gutter + plan.slack;
}

/** The cards a page holds, as `[start, end)` indices into the hand. */
export function fanPageRange(plan: FanPlan, page: number): { start: number; end: number } {
  if (plan.pages === 0) return { start: 0, end: 0 };
  const clamped = Math.max(0, Math.min(page, plan.pages - 1));
  const start = clamped * plan.pageSize;
  return { start, end: Math.min(plan.count, start + plan.pageSize) };
}

/** Which page the card at `index` sits on. */
export function fanPageOf(plan: FanPlan, index: number): number {
  if (plan.pages <= 1 || plan.pageSize <= 0) return 0;
  return Math.max(0, Math.min(plan.pages - 1, Math.floor(index / plan.pageSize)));
}

/**
 * The receiver's fan for a hand band of `bandWidth` px.
 *
 * Two passes, and the second only runs when the first says the hand pages: the
 * page controls claim {@link FAN.pagerW} on each side, and reserving that space
 * for a hand that fits would page a seven-card opening hand on phone portrait,
 * where the band is exactly wide enough without it. Re-planning in the reduced
 * span can only ever *increase* the page count, so the two passes cannot
 * oscillate.
 */
export function localFanPlan(
  count: number,
  bandWidth: number,
): { plan: FanPlan; paged: boolean; span: number } {
  const first = fanPlan(count, bandWidth, LOCAL_FAN_TIER);
  if (first.pages <= 1) return { plan: first, paged: false, span: bandWidth };
  const span = Math.max(0, bandWidth - 2 * FAN.pagerW);
  return { plan: fanPlan(count, span, LOCAL_FAN_TIER), paged: true, span };
}
