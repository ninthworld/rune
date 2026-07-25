import type { PlayerId } from '../../protocol';
import type { Rect, SurfaceTier } from '../scene/types';
import type { PlaneViewport, WingSide } from './types';
import { PLANE } from './metrics';

/** One carved wing slot, before content staging. */
export interface WingSlotFrame {
  /** The peripheral seat staged here. */
  seat: PlayerId;
  /** The wing's slot rect (may bleed past the plane edge, never into the corridor). */
  rect: Rect;
  /** Which side of the plane the wing sits on. */
  side: WingSide;
  /** Wing row from the top, 0-based. */
  rank: number;
  /** The wing's baseline surface tier. */
  surface: SurfaceTier;
  /**
   * Whether the wing stages at the digest rung from the start (two wings per
   * side, 5–6 players — layout-model §Staging per player count). A wing that is
   * not digest-baseline may still reach the digest rung down the ladder.
   */
  digestBaseline: boolean;
}

/** The carved fixed slots for one staging pass. */
export interface PlaneSlotFrames {
  /** The receiver's band rect; absent when the receiver is unknown. */
  receiver?: Rect;
  /** The far-side rect and its baseline surface; absent with no opponents. */
  far?: { rect: Rect; surface: SurfaceTier };
  /** Wing slots, in stable seat order. */
  wings: WingSlotFrame[];
  /** Summary-tile rects (compact branch only), in stable seat order. */
  tiles: { seat: PlayerId; rect: Rect }[];
  /** The center corridor, clear by construction. */
  corridor: Rect;
}

/**
 * The rect slots are carved inside — the viewport's {@link PlaneViewport.safe}
 * staging box, or the whole plane when no chrome is standing on it.
 *
 * Every fraction below is a fraction of the *staging box*, not of the plane. A
 * slot fraction that resolved against the raw viewport would stage the
 * receiver's band underneath the hand fan and the wings behind the control
 * cluster, which is the exact failure #534 forbids ("never cover a candidate,
 * selected card, player cluster, or required path endpoint").
 */
function stagingBox(viewport: PlaneViewport): Rect {
  return viewport.safe ?? { x: 0, y: 0, w: viewport.width, h: viewport.height };
}

/**
 * Carve the plane's fixed slots (layout-model §The plane and its fixed slots):
 * the receiver's full-width bottom band, the far side, the wings staged outward
 * from the top (alternating left/right in the given stable seat order), and the
 * clear center corridor between the far side and the receiver's band.
 *
 * All of it happens inside the viewport's staging box (see {@link stagingBox}).
 * The wing bleed and the crest overhang measure from the **box's** edges, not
 * the plane's, so "tucked partway offstage" stays a constant fraction of the
 * wing rather than growing as chrome takes more of the viewport.
 */
export function carveSlots(
  viewport: PlaneViewport,
  hasReceiver: boolean,
  farSeat: PlayerId | undefined,
  peripherals: PlayerId[],
): PlaneSlotFrames {
  const box = stagingBox(viewport);
  const { w: W, h: H } = box;
  const receiverH = H * PLANE.receiver.h;
  const receiver: Rect | undefined = hasReceiver
    ? {
        x: box.x + W * PLANE.receiver.x,
        y: box.y + H - receiverH,
        w: W * PLANE.receiver.w,
        h: receiverH,
      }
    : undefined;

  const duel = peripherals.length === 0;
  const farSpec = duel ? PLANE.duelFar : PLANE.far;

  // Ultrawide surplus-width policy (layout-model §Hand-offs and open items):
  // beyond `corridorMaxAspect` the multiplayer far side and the center corridor
  // stop widening. The central column is capped at `H × corridorMaxAspect` and
  // centered; the surplus horizontal width falls into the side gutters, where
  // the wings (still full-width fractions of W below) spend it — surplus goes to
  // the wings before the corridor. A duel keeps its full-width far side (no
  // wings to spend surplus on).
  const centralW = !duel && W > H * PLANE.corridorMaxAspect ? H * PLANE.corridorMaxAspect : W;
  const centralX = box.x + (W - centralW) / 2;
  const farX = centralX + centralW * farSpec.x;
  const farW = centralW * farSpec.w;
  const far =
    farSeat === undefined
      ? undefined
      : {
          rect: { x: farX, y: box.y + H * farSpec.y, w: farW, h: H * farSpec.h },
          surface: (duel ? 'field' : 'support') as SurfaceTier,
        };

  // Up to two wings per side, alternating left/right in seat order; a wing whose
  // slot is narrower than the digest threshold (`digestBelowWidthFrac`) stages
  // at the smaller, digest-baseline slot — two-per-side staging by construction.
  const perSide = Math.ceil(peripherals.length / 2);
  const spec = perSide > 1 ? PLANE.wing.double : PLANE.wing.single;
  const digestBaseline = spec.w < PLANE.wing.digestBelowWidthFrac;
  const w = W * spec.w;
  const h = H * spec.h;
  // One-per-side staging hangs its lone rank at mid height, the way the baseline
  // arena does; two-per-side keeps the top anchor so the pair spans the flank.
  const top = perSide > 1 ? PLANE.wing.top : PLANE.wing.singleTop;
  const wings: WingSlotFrame[] = peripherals.map((seat, i) => {
    const side: WingSide = i % 2 === 0 ? 'left' : 'right';
    const rank = Math.floor(i / 2);
    const x =
      side === 'left' ? box.x - w * PLANE.wing.bleed : box.x + W - w * (1 - PLANE.wing.bleed);
    const y = box.y + H * top + rank * (h + H * PLANE.wing.rankGap);
    return {
      seat,
      rect: { x, y, w, h },
      side,
      rank,
      surface: digestBaseline ? 'mini' : 'support',
      digestBaseline,
    };
  });

  // The corridor spans the (capped) far side's width, from its bottom edge down
  // to the receiver's band. Wing inner edges stay outside it via the plane-edge
  // bleed, and the ultrawide surplus widens the gutter between them.
  const farBottom = far ? far.rect.y + far.rect.h : box.y;
  const corridor: Rect = {
    x: farX,
    y: farBottom,
    w: farW,
    h: Math.max(0, (receiver ? receiver.y : box.y + H) - farBottom),
  };

  return { receiver, far, wings, tiles: [], corridor };
}

/**
 * Carve the compact change-of-kind slots (rung 5, phone portrait, 3+ players):
 * the receiver keeps the full bottom anatomy, the focused opponent keeps a drawn
 * board at the top, and every other opponent collapses to a ≥ 44 px summary
 * tile. The corridor is the tile-free band beside the tile column.
 */
export function carveCompactSlots(
  viewport: PlaneViewport,
  peripherals: PlayerId[],
): PlaneSlotFrames {
  const box = stagingBox(viewport);
  const { w: W, h: H } = box;
  const receiverH = H * PLANE.compact.receiverH;
  const receiver: Rect = {
    x: box.x + W * 0.06,
    y: box.y + H - receiverH,
    w: W * 0.88,
    h: receiverH,
  };
  const spec = PLANE.compact.far;
  const far = {
    // `spec.y` is a flat px drop, not a fraction — the compact far side hangs a
    // fixed distance below the box's top edge rather than a share of its height.
    rect: { x: box.x + W * spec.x, y: box.y + spec.y, w: W * spec.w, h: H * spec.h },
    surface: 'mini' as SurfaceTier,
  };
  const t = PLANE.compact.tile;
  const farBottom = far.rect.y + far.rect.h;
  let y = farBottom + t.topGap;
  const tiles = peripherals.map((seat) => {
    const rect: Rect = { x: box.x + W * t.x, y, w: W * t.w, h: t.h };
    y += t.h + t.gap;
    return { seat, rect };
  });
  const corridorX = box.x + W * t.x + W * t.w + t.stripGap;
  const corridor: Rect = {
    x: corridorX,
    y: farBottom,
    w: Math.max(0, box.x + W * 0.98 - corridorX),
    h: Math.max(0, receiver.y - farBottom),
  };
  return { receiver, far, wings: [], tiles, corridor };
}
