/**
 * The per-seat **zone rack** — library, graveyard, exile, and command as
 * physical objects on the seat's outer flank, implementing
 * `docs/design/zone-geography.md` §2 (frame, anchor table, pitch, corridor
 * clearance) and §6 (the four variants) for issue #531.
 *
 * Pure geometry. Nothing here reads the DOM, computes legality, or decides what
 * a pile *shows*: it answers only where each of a seat's four zone anchors sits,
 * how big it is, and whether the rack can draw four separable targets at all.
 * The counts ride through untouched from `zoneCountsOf`.
 *
 * Four rules from the specification drive every number below:
 *
 * - **Order is fixed and never reverses** (§1 fact 1): library, graveyard,
 *   exile along the reading axis, always in that order, at every seat.
 * - **Nothing is rotated** (§1 fact 2) and **the strip sits screen-right of the
 *   anchor** on every vertical rack, screen-below it on every horizontal one
 *   (§1 fact 3, [D1]). The rack is never mirrored; only the *command* slot
 *   changes side.
 * - **The command slot is always inboard** (§1 fact 4, §2.2): where the strip is
 *   already inboard of the identity anchor the command slot clears it
 *   (`+3.35u`); where the strip is outboard — a right-flank seat, exactly as the
 *   baseline's two right-hand clusters are drawn — the command slot crosses to
 *   the anchor's far side (`−2.00u`) and is still the innermost element.
 * - **`u ≥ 30 px` or digest** (§2.3 [D4]): below that the along-axis pitch falls
 *   under 48 px and three 44 px hit rects can no longer be separated. That is
 *   the single numeric trigger, and it is checked against the *fitted* `u`, so a
 *   rack degrades rather than shrinking the board it belongs to.
 *
 * A rack **never trims a zone away** (§2.4.2): one that cannot satisfy the
 * corridor-clearance rule inside its region digests, keeping all four anchors.
 */
import type { PlayerId } from '../../protocol';
import type { Rect, ZoneCounts } from '../scene/types';
import { PLANE, hitRectFor } from './metrics';
import type { PlaneRegionKind, PlaneViewport, WingSide } from './types';

/** The four zone anchors every seat reserves (§I6 — no fifth slot, ever). */
export const RACK_ZONES = ['library', 'graveyard', 'exile', 'command'] as const;

/** One of a seat's four zone anchors. */
export type RackZone = (typeof RACK_ZONES)[number];

/**
 * Which rack anatomy a seat draws (§6). `local`/`focused`/`wing` are the same
 * four slots at three scales; `digest` is one ≥ 44 px button carrying four
 * shaped sub-indicators, which every zone key then resolves to (§7).
 */
export type RackVariant = 'local' | 'focused' | 'wing' | 'digest';

/** One drawn zone slot and its hotspot. */
export interface RackSlot {
  /** Which zone this slot anchors. */
  zone: RackZone;
  /** The drawn footprint. */
  rect: Rect;
  /** The hotspot: `rect` grown to the 44 px floor (§2.3, drawn box unchanged). */
  hitRect: Rect;
  /** The zone's count, straight from the view (§4 — one home per datum). */
  count: number;
}

/** A seat's staged zone rack. */
export interface SeatRack {
  /** The seat this rack belongs to. */
  seat: PlayerId;
  /** Which anatomy it drew (§6). */
  variant: RackVariant;
  /** The reading axis: `along` runs down a vertical rack, right across a horizontal one. */
  axis: 'vertical' | 'horizontal';
  /** The pile-card width `u` the rack fitted to, in px (0 for a digest rack). */
  u: number;
  /** The identity anchor the §2.2 offsets are measured from (the medallion centre). */
  origin: { x: number; y: number };
  /** The slots, in the fixed §1 order. A rack without a command slot has three. */
  slots: RackSlot[];
  /** The union of every slot's hit rect — the `zone:<seat>:rack` anchor (§7). */
  bounds: Rect;
  /** How far the region's card content must be inset to clear the rack, per edge. */
  inset: { left: number; right: number };
}

/** Everything `stageRack` needs; all of it already derived by the stage. */
export interface RackRequest {
  /** The seat. */
  seat: PlayerId;
  /** Which fixed slot group the seat occupies (drives axis and outer edge). */
  kind: PlaneRegionKind;
  /** Wing side, for a wing (drives which flank is outboard). */
  side?: WingSide;
  /** The region's slot rect (a wing's may bleed past the plane edge). */
  rect: Rect;
  /** The plane, so a bleeding wing's rack stays on-canvas. */
  viewport: PlaneViewport;
  /** The seat's zone counts. */
  zones: ZoneCounts;
  /** Whether this game has a command zone at all (§5 / §12 gap G3). */
  commander: boolean;
  /** Whether the seat's board is a digest baseline (§6 — a digest board digests its rack). */
  digestBaseline: boolean;
  /** The centre corridor the rack must clear by {@link PLANE.rack.halo} (§2.4). */
  corridor: Rect;
}

/** The along-axis span in `u`: anchor's leading edge to the exile slot's trailing edge. */
function alongSpan(horizontal: boolean): number {
  // The along axis measures the pile's width on a horizontal rack and its
  // height on a vertical one — the same silhouette, read on the other axis.
  const halfTrailing = horizontal ? 0.5 : PLANE.rack.pileAspect / 2;
  return PLANE.rack.medallion / 2 + 2 * PLANE.rack.pitch + halfTrailing;
}

/**
 * The command slot's perpendicular offset, in `u`, for an **inboard** strip.
 *
 * §2.2 fixes it at `+3.35u`, a value drawn from the baseline's four *vertical*
 * clusters, and says the rule "generalizes to horizontal racks". It does not, at
 * that number: on a horizontal rack the perpendicular axis measures the pile's
 * **height** (`1.4u`) and the command slot's (`1.89u`), so `3.35u` leaves the
 * two overlapping and §2.3's packing rule fails — which would digest every
 * focused seat's rack in a Commander game. The offset is therefore the larger of
 * the specified value and the smallest that separates the two slots. On a
 * vertical rack the specified `3.35u` already wins, so the baseline's number is
 * reproduced exactly and only the horizontal case moves. Reported as a
 * specification gap with issue #531.
 */
function commandPerpU(horizontal: boolean): number {
  const halfPile = horizontal ? PLANE.rack.pileAspect / 2 : 0.5;
  const halfCommand = (PLANE.rack.commandScale * (horizontal ? PLANE.rack.pileAspect : 1)) / 2;
  // …plus a little breathing room, so the two slots are separated rather than
  // exactly touching (§2.3 forbids overlap, and an exactly-touching pair is one
  // float rounding away from failing it).
  return Math.max(
    PLANE.rack.commandInboard,
    PLANE.rack.strip + halfPile + halfCommand + PLANE.rack.commandClear,
  );
}

/** The command slot's half-extent perpendicular to the reading axis, in `u`. */
function commandHalfPerpU(horizontal: boolean): number {
  return (PLANE.rack.commandScale * (horizontal ? PLANE.rack.pileAspect : 1)) / 2;
}

/**
 * How far the cluster reaches on each side of the identity anchor, perpendicular
 * to the reading axis, in `u`. `behind` is the negative-perp reach, `ahead` the
 * positive-perp reach; the pile strip is always at `+strip`.
 */
function perpReach(
  commander: boolean,
  outboard: boolean,
  horizontal: boolean,
): { behind: number; ahead: number } {
  const anchor = PLANE.rack.medallion / 2;
  const stripFar = PLANE.rack.strip + (horizontal ? PLANE.rack.pileAspect / 2 : 0.5);
  const half = commandHalfPerpU(horizontal);
  if (!commander) return { behind: anchor, ahead: stripFar };
  return outboard
    ? { behind: Math.max(anchor, -PLANE.rack.commandOutboard + half), ahead: stripFar }
    : { behind: anchor, ahead: Math.max(stripFar, commandPerpU(horizontal) + half) };
}

/** A rect clipped to the plane, so a bleeding wing's rack stays reachable. */
function onPlane(rect: Rect, viewport: PlaneViewport): Rect {
  const x0 = Math.max(0, rect.x);
  const y0 = Math.max(0, rect.y);
  const x1 = Math.min(viewport.width, rect.x + rect.w);
  const y1 = Math.min(viewport.height, rect.y + rect.h);
  return { x: x0, y: y0, w: Math.max(0, x1 - x0), h: Math.max(0, y1 - y0) };
}

/** Whether two rects share positive area. */
function overlaps(a: Rect, b: Rect): boolean {
  return a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h;
}

/** The union of a non-empty list of rects. */
function union(rects: Rect[]): Rect {
  const x0 = Math.min(...rects.map((r) => r.x));
  const y0 = Math.min(...rects.map((r) => r.y));
  const x1 = Math.max(...rects.map((r) => r.x + r.w));
  const y1 = Math.max(...rects.map((r) => r.y + r.h));
  return { x: x0, y: y0, w: x1 - x0, h: y1 - y0 };
}

/** The zone counts in the fixed §1 order, command last. */
function countsFor(zones: ZoneCounts, commander: boolean): { zone: RackZone; count: number }[] {
  const rows: { zone: RackZone; count: number }[] = [
    { zone: 'library', count: zones.library },
    { zone: 'graveyard', count: zones.graveyard },
    { zone: 'exile', count: zones.exile },
  ];
  if (commander) rows.push({ zone: 'command', count: zones.command ?? 0 });
  return rows;
}

/** Which region kind maps to which drawn variant (§6). */
function variantFor(kind: PlaneRegionKind): Exclude<RackVariant, 'digest'> {
  if (kind === 'receiver') return 'local';
  return kind === 'far' ? 'focused' : 'wing';
}

/** The variant's nominal pile width before fitting (§6: local field, focused one
 * tier down, wing mini). */
function nominalFor(kind: PlaneRegionKind): number {
  if (kind === 'receiver') return PLANE.rack.nominal.receiver;
  return kind === 'far' ? PLANE.rack.nominal.far : PLANE.rack.nominal.wing;
}

/** The share of its region the rack may claim perpendicular to its reading axis. */
function shareFor(kind: PlaneRegionKind): number {
  if (kind === 'receiver') return PLANE.rack.share.receiver;
  return kind === 'far' ? PLANE.rack.share.far : PLANE.rack.share.wing;
}

/**
 * Stage one seat's zone rack.
 *
 * The rack is anchored to its region's **outer edge** and grows inboard from it
 * (§2.4.1). Which edge that is comes from the region kind, exactly as §2.5's
 * table sets out: the receiver band's left flank (vertical rack), the far side's
 * top edge (horizontal rack), and a wing's own plane edge (vertical rack, with
 * the strip outboard of the anchor on the right-hand flank). `u` is then the
 * largest pile width that fits the cluster inside the region's share of the
 * slot, capped at the variant's nominal tier width; below
 * {@link PLANE.rack.minU}, or when packing or clearance fails, the rack digests.
 */
export function stageRack(request: RackRequest): SeatRack {
  const { seat, kind, side, viewport, zones, commander, corridor } = request;
  const region = onPlane(request.rect, viewport);
  // The rack sits inside its region by its own padding **plus** the §2.4.2
  // clearance halo, so the halo itself is contained in the region rect. Regions
  // never overlap the corridor, which makes corridor clearance hold by
  // construction rather than by luck; the explicit check below stays as the
  // guard that says so.
  const pad = PLANE.pad + PLANE.rack.halo;
  const horizontal = kind === 'far';
  const axis: SeatRack['axis'] = horizontal ? 'horizontal' : 'vertical';
  // A right-flank wing is the one staging where the strip's fixed screen-right
  // direction points *away* from the table — §2.2's outboard command branch.
  const outboard = kind === 'wing' && side === 'right';

  const reach = perpReach(commander, outboard, horizontal);
  const span = reach.behind + reach.ahead;
  const alongBudget = (horizontal ? region.w * shareFor(kind) : region.h) - 2 * pad;
  const perpBudget = horizontal ? region.h - 2 * pad : region.w * shareFor(kind);
  const u = Math.floor(
    Math.min(
      nominalFor(kind),
      alongBudget / alongSpan(horizontal),
      perpBudget / Math.max(span, 0.001),
    ),
  );
  if (request.digestBaseline || u < PLANE.rack.minU)
    return digestRack(request, region, axis, outboard);

  // The anchor sits so the whole cluster hugs the region's outer edge: the
  // along axis always starts at the region's leading edge; the perpendicular
  // axis starts at the outer flank, which is the far edge for an outboard rack.
  const alongStart = (horizontal ? region.x : region.y) + pad + (PLANE.rack.medallion / 2) * u;
  const perpStart = outboard
    ? region.x + region.w - pad - reach.ahead * u
    : (horizontal ? region.y : region.x) + pad + reach.behind * u;
  const origin = horizontal ? { x: alongStart, y: perpStart } : { x: perpStart, y: alongStart };

  const place = (along: number, perp: number, w: number, h: number): Rect => {
    const cx = horizontal ? origin.x + along * u : origin.x + perp * u;
    const cy = horizontal ? origin.y + perp * u : origin.y + along * u;
    return { x: cx - w / 2, y: cy - h / 2, w, h };
  };

  const pileH = u * PLANE.rack.pileAspect;
  const commandW = u * PLANE.rack.commandScale;
  const commandPerp = outboard ? PLANE.rack.commandOutboard : commandPerpU(horizontal);
  const slots: RackSlot[] = countsFor(zones, commander).map(({ zone, count }, index) => {
    const rect =
      zone === 'command'
        ? place(PLANE.rack.commandAlong, commandPerp, commandW, commandW * PLANE.rack.pileAspect)
        : place(index * PLANE.rack.pitch, PLANE.rack.strip, u, pileH);
    return { zone, rect, hitRect: hitRectFor(rect), count };
  });

  const bounds = union(slots.map((slot) => slot.hitRect));
  // §2.3: no two hit rects in one rack may overlap. §2.4.2: the hit-rect union
  // plus a 12 px halo must not intersect the corridor. Either failure digests.
  const packed = slots.every((slot, i) =>
    slots.every((other, j) => i === j || !overlaps(slot.hitRect, other.hitRect)),
  );
  const halo: Rect = {
    x: bounds.x - PLANE.rack.halo,
    y: bounds.y - PLANE.rack.halo,
    w: bounds.w + 2 * PLANE.rack.halo,
    h: bounds.h + 2 * PLANE.rack.halo,
  };
  if (!packed || overlaps(halo, corridor)) return digestRack(request, region, axis, outboard);

  return {
    seat,
    variant: variantFor(kind),
    axis,
    u,
    origin,
    slots,
    bounds,
    inset: insetFor(outboard, request.rect, bounds),
  };
}

/**
 * The digest rack (§6.1): one ≥ 44 px button on the region's outer edge. Every
 * zone key resolves to it (§7), so an anchor is never lost — the four counts
 * ride as sub-indicators, which are a paint concern, not geometry.
 */
function digestRack(
  request: RackRequest,
  region: Rect,
  axis: SeatRack['axis'],
  outboard: boolean,
): SeatRack {
  const { seat, zones, commander } = request;
  const pad = PLANE.pad + PLANE.rack.halo;
  const w = Math.max(PLANE.minHit, PLANE.pile.w);
  const h = Math.max(PLANE.minHit, PLANE.pile.h);
  const rect: Rect = {
    x: outboard ? region.x + region.w - pad - w : region.x + pad,
    y: region.y + pad,
    w,
    h,
  };
  const hitRect = hitRectFor(rect);
  const slots: RackSlot[] = countsFor(zones, commander).map(({ zone, count }) => ({
    zone,
    rect,
    hitRect,
    count,
  }));
  return {
    seat,
    variant: 'digest',
    axis,
    u: 0,
    origin: { x: rect.x + w / 2, y: rect.y + h / 2 },
    slots,
    bounds: hitRect,
    inset: insetFor(outboard, request.rect, hitRect),
  };
}

/**
 * How far the region's content must step aside for the rack. The board never
 * loses height to a rack — a horizontal rack runs *along* the region's outer
 * edge from its leading end, so both orientations cost the same axis.
 */
function insetFor(outboard: boolean, slot: Rect, bounds: Rect): { left: number; right: number } {
  const gap = PLANE.rack.gap;
  return outboard
    ? { left: 0, right: Math.max(0, slot.x + slot.w - bounds.x) + gap }
    : { left: Math.max(0, bounds.x + bounds.w - slot.x) + gap, right: 0 };
}
