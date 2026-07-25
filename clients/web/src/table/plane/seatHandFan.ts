/**
 * An opponent seat's **face-down hand fan** on the plane (issue #533, widened
 * scope; `card-representation.md` §13, `zone-geography.md` §4.1).
 *
 * `zone-geography.md` §4.1 argues the hand count belongs on the identity
 * cluster because *"the hand has no pile; the library does. An opponent's hand
 * renders as a face-down fan with no count-bearing surface"* — a sentence that
 * rested on a fan nobody had built. This module builds it, and the baseline
 * draws it: `rune-2.5d-interface-baseline.jpg` gives Veyra a fan of backs above
 * her portrait and both wing seats one too.
 *
 * ## Hidden-information safety is the module's shape, not a review note
 *
 * The whole surface is generated from **one number**. {@link stageSeatHandFan}
 * is handed a count, a cluster, a rack, and a viewport; it is never handed a
 * card, an entity id, a zone's contents, or a `GameView`, and a
 * {@link HandFanSlot} carries no field that could hold one. Every slot's rect
 * and rotation comes from `table/handFan.ts`'s curve, whose only inputs are
 * `(index, count, tier)`. So a back cannot vary with the card it hides — not by
 * art, not by rotation, not by width, not by order — because nothing on this
 * path is ever told what that card is. That is the same argument `card/back/`
 * makes for the image itself, and `seatHandFan.test.ts` asserts it structurally.
 *
 * The image is the one the device already resolved: the fan paints
 * `--card-back-image`, published once on the plane root by `LivePlane`. There
 * is no second card-back path.
 *
 * ## Placement
 *
 * The fan is a seat fixture, like the crest and the rack, so it stages at every
 * rung where the seat's region is drawn — the focused far side and every wing,
 * full-board or digest. It sits **above the identity cluster's portrait**, which
 * the medallion overlaps by its lower edge exactly as the baseline draws it, and
 * falls **below** the cluster when the space above is taken by the seat's own
 * zone rack or would leave the plane — the "try the baseline placement, then the
 * smallest fix" idiom `cluster.ts` uses for the nameplate.
 *
 * Rung 5's summary tiles get no fan, deliberately: the tile *is* the minimal
 * cluster rung (`zone-geography.md` §4.1), it draws no hand pip either, its own
 * row already carries the hand count as text, and its growth budget is reserved
 * for candidate strips, which are load-bearing picks. The `hand:<seat>` anchor
 * still resolves for a tile seat — to the tile — so §9's draw motion terminates.
 */
import type { PlayerId } from '../../protocol';
import type { Rect } from '../scene/types';
import {
  fanAngle,
  fanDip,
  fanFraction,
  fanInset,
  fanPageRange,
  fanPlan,
  opponentFanSpan,
  opponentFanTier,
  type FanPlan,
  type FanTier,
  type HandCountBand,
} from '../handFan';
import { clampToEnvelope } from './metrics';
import type { PlaneViewport } from './types';

/**
 * How the fan sits relative to the cluster it belongs to. Fractions of the
 * cluster's scale unit `D`, so the fan scales with the rung exactly as every
 * other cluster element does (`seat-identity.md` §1.1).
 */
export const HAND_FAN = {
  /** How far the medallion overlaps the fan's near edge, in `D`. */
  overlap: 0.34,
  /** Clear gap kept between the fan and the seat's zone rack, in px. */
  rackGap: 6,
  /** Margin the fan keeps off the plane's edges, in px. */
  margin: 2,
} as const;

/**
 * One drawn back. Its **entire** contents: where the slot is, how far it is
 * turned, and which slot it is. There is no field here a card could reach.
 */
export interface HandFanSlot {
  /** The slot's 0-based place in the fan. Never an entity id. */
  index: number;
  /** The back's drawn box. */
  rect: Rect;
  /** The slot's rotation, in degrees — a function of `(index, count)` alone. */
  angleDeg: number;
}

/** One seat's staged face-down hand fan. */
export interface SeatHandFan {
  /** The seat the fan belongs to. */
  seat: PlayerId;
  /** The seat's hand size, straight from the view — the fan's only datum. */
  count: number;
  /** The count band the fan resolved (`handFan.ts` bands). */
  band: HandCountBand;
  /** The card box every back in this fan draws at. */
  card: { w: number; h: number };
  /**
   * The backs actually drawn: `min(count, capacity)`. A hand deeper than the
   * fan's span holds renders its first page and stops — the same paging
   * mechanism the receiver's hand uses, in its non-interactive form, where
   * there is nothing to page *to*. The authoritative count is the cluster's
   * hand pip, which is the count's one home (`zone-geography.md` §4/I5).
   */
  slots: HandFanSlot[];
  /** Cards the fan could not draw (`count - slots.length`; normally `0`). */
  undrawn: number;
  /** The union of every drawn back, plus the empty fan's reserved box. */
  bounds: Rect;
  /**
   * The `hand:<seat>` travel anchor (`zone-geography.md` §7, §9): the slot a
   * drawn card lands on, so a draw terminates on a real fan slot rather than
   * falling back to the seat crest. Present even for an empty hand.
   */
  anchor: Rect;
}

/** Everything the fan needs. Note what is absent: cards, ids, zones, views. */
export interface SeatHandFanRequest {
  /** The seat, for the fan's anchor and element keys only. */
  seat: PlayerId;
  /** The seat's hand size. */
  count: number;
  /** The cluster's scale unit `D` — the fan's own scale. */
  d: number;
  /** The cluster's portrait medallion box, which the fan sits above. */
  portrait: Rect;
  /** The seat's zone rack bounds; the fan steps around it rather than through it. */
  keepOut?: Rect;
  /** The plane, so nothing stages off-canvas. */
  viewport: PlaneViewport;
}

/** Whether two rects share positive area. */
function overlaps(a: Rect, b: Rect): boolean {
  return a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h;
}

/**
 * Lay `plan`'s first page out inside a box whose top-left is `origin`. Pure
 * geometry over `(index, count)`; the loop never sees anything else.
 */
function layOut(origin: { x: number; y: number }, plan: FanPlan, tier: FanTier): HandFanSlot[] {
  const { start, end } = fanPageRange(plan, 0);
  const inset = fanInset(plan, tier);
  const shown = end - start;
  const slots: HandFanSlot[] = [];
  for (let i = 0; i < shown; i += 1) {
    const t = fanFraction(i, shown);
    const centre = origin.x + inset + (plan.span - 2 * inset) * t;
    slots.push({
      index: i,
      rect: {
        x: Math.round(centre - tier.card.w / 2),
        y: Math.round(origin.y + fanDip(i, shown, tier)),
        w: tier.card.w,
        h: tier.card.h,
      },
      angleDeg: Number(fanAngle(i, shown, tier).toFixed(2)),
    });
  }
  return slots;
}

/** The union of a rect list, or `fallback` when it is empty. */
function union(rects: Rect[], fallback: Rect): Rect {
  if (rects.length === 0) return fallback;
  const x0 = Math.min(...rects.map((r) => r.x));
  const y0 = Math.min(...rects.map((r) => r.y));
  const x1 = Math.max(...rects.map((r) => r.x + r.w));
  const y1 = Math.max(...rects.map((r) => r.y + r.h));
  return { x: x0, y: y0, w: x1 - x0, h: y1 - y0 };
}

/**
 * Stage one opponent seat's face-down hand fan.
 *
 * The fan is always produced, even for an empty hand: it publishes the
 * `hand:<seat>` anchor, and a draw into an empty hand has to terminate
 * somewhere real.
 */
export function stageSeatHandFan(request: SeatHandFanRequest): SeatHandFan {
  const { seat, count, d, portrait, keepOut, viewport } = request;
  const tier = opponentFanTier(d);
  const span = opponentFanSpan(d);
  const plan = fanPlan(count, span, tier);
  // The arc's deepest dip decides the fan's height; the box has to hold it or
  // the outermost backs are laid outside their own bounds.
  const dip = tier.arcFrac * tier.card.h;
  const boxH = Math.ceil(tier.card.h + dip);
  const left = portrait.x + portrait.w / 2 - span / 2;

  const box = (y: number): Rect => ({ x: left, y, w: span, h: boxH });
  // The baseline placement first — above the medallion, whose lower edge
  // overlaps the fan — then the smallest fix, in order: slide clear of the top
  // of the seat's own zone rack, slide clear of its bottom, and finally hang
  // the fan below the medallion. This is `cluster.ts`'s nameplate idiom: the
  // baseline treatment returns the moment nothing is in the way.
  const above = portrait.y + HAND_FAN.overlap * d - boxH;
  const tries = [
    above,
    ...(keepOut === undefined
      ? []
      : [
          keepOut.y - HAND_FAN.rackGap - boxH,
          keepOut.y + keepOut.h + HAND_FAN.rackGap,
          portrait.y + portrait.h - HAND_FAN.overlap * d,
        ]),
  ];
  let clamped = clampToEnvelope(box(above), viewport);
  for (const y of tries) {
    const candidate = clampToEnvelope(box(y), viewport);
    if (keepOut === undefined || !overlaps(candidate, keepOut)) {
      clamped = candidate;
      break;
    }
  }
  const slots = layOut({ x: clamped.x, y: clamped.y }, plan, tier);
  const bounds = union(
    slots.map((slot) => slot.rect),
    clamped,
  );
  const last = slots[slots.length - 1];
  const anchor = last?.rect ?? {
    x: Math.round(clamped.x + clamped.w / 2 - tier.card.w / 2),
    y: Math.round(clamped.y),
    w: tier.card.w,
    h: tier.card.h,
  };
  return {
    seat,
    count,
    band: plan.band,
    card: tier.card,
    slots,
    undrawn: Math.max(0, count - slots.length),
    bounds,
    anchor,
  };
}
