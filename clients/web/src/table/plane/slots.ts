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
 * Carve the plane's fixed slots (layout-model §The plane and its fixed slots):
 * the receiver's full-width bottom band, the far side, the wings staged outward
 * from the top (alternating left/right in the given stable seat order), and the
 * clear center corridor between the far side and the receiver's band.
 */
export function carveSlots(
  viewport: PlaneViewport,
  hasReceiver: boolean,
  farSeat: PlayerId | undefined,
  peripherals: PlayerId[],
): PlaneSlotFrames {
  const { width: W, height: H } = viewport;
  const receiverH = H * PLANE.receiver.h;
  const receiver: Rect | undefined = hasReceiver
    ? { x: W * PLANE.receiver.x, y: H - receiverH, w: W * PLANE.receiver.w, h: receiverH }
    : undefined;

  const duel = peripherals.length === 0;
  const farSpec = duel ? PLANE.duelFar : PLANE.far;
  const far =
    farSeat === undefined
      ? undefined
      : {
          rect: { x: W * farSpec.x, y: H * farSpec.y, w: W * farSpec.w, h: H * farSpec.h },
          surface: (duel ? 'field' : 'support') as SurfaceTier,
        };

  // Up to two wings per side, alternating left/right in seat order; two per
  // side drops the wing to the smaller, digest-baseline slot.
  const perSide = Math.ceil(peripherals.length / 2);
  const spec = perSide > 1 ? PLANE.wing.double : PLANE.wing.single;
  const w = W * spec.w;
  const h = H * spec.h;
  const wings: WingSlotFrame[] = peripherals.map((seat, i) => {
    const side: WingSide = i % 2 === 0 ? 'left' : 'right';
    const rank = Math.floor(i / 2);
    const x = side === 'left' ? -w * PLANE.wing.bleed : W - w * (1 - PLANE.wing.bleed);
    const y = H * PLANE.wing.top + rank * (h + H * PLANE.wing.rankGap);
    return {
      seat,
      rect: { x, y, w, h },
      side,
      rank,
      surface: perSide > 1 ? 'mini' : 'support',
      digestBaseline: perSide > 1,
    };
  });

  // The corridor spans the far side's width, from its bottom edge down to the
  // receiver's band. Wing inner edges stay outside it via the plane-edge bleed.
  const farBottom = far ? far.rect.y + far.rect.h : 0;
  const corridor: Rect = {
    x: W * farSpec.x,
    y: farBottom,
    w: W * farSpec.w,
    h: Math.max(0, (receiver ? receiver.y : H) - farBottom),
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
  const { width: W, height: H } = viewport;
  const receiverH = H * PLANE.compact.receiverH;
  const receiver: Rect = { x: W * 0.06, y: H - receiverH, w: W * 0.88, h: receiverH };
  const spec = PLANE.compact.far;
  const far = {
    rect: { x: W * spec.x, y: spec.y, w: W * spec.w, h: H * spec.h },
    surface: 'mini' as SurfaceTier,
  };
  const t = PLANE.compact.tile;
  const farBottom = far.rect.y + far.rect.h;
  let y = farBottom + t.topGap;
  const tiles = peripherals.map((seat) => {
    const rect: Rect = { x: W * t.x, y, w: W * t.w, h: t.h };
    y += t.h + t.gap;
    return { seat, rect };
  });
  const corridorX = W * t.x + W * t.w + t.stripGap;
  const corridor: Rect = {
    x: corridorX,
    y: farBottom,
    w: Math.max(0, W * 0.98 - corridorX),
    h: Math.max(0, receiver.y - farBottom),
  };
  return { receiver, far, wings: [], tiles, corridor };
}
