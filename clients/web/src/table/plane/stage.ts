import type { GameView, Permanent, PlayerId } from '../../protocol';
import { CARD_BOX } from '../../card/dom';
import type { Rect, SurfaceTier } from '../scene/types';
import {
  localPlayerIdOf,
  orderedOpponentIds,
  bandLabel,
  zoneCountsOf,
} from '../scene/band-helpers';
import { seatIdentityFacts, attackerCountOn, seatAccent, seatOrderOf } from '../seatIdentity';
import { portraitFor } from '../seatPortraits';
import { PLANE, isCompactGeometry, hitRectFor, clampToEnvelope } from './metrics';
import { carveSlots, carveCompactSlots, type WingSlotFrame } from './slots';
import { resolveFocusSeat } from './focus';
import { buildStageItems, stageRegionContent, type StageItem } from './regions';
import { stageRack, type SeatRack } from './rack';
import { clusterD, stageSeatCluster, type ClusterVariant } from './cluster';
import { stageSeatHandFan } from './seatHandFan';
import type {
  PlaneViewport,
  PlaneStagingState,
  PlaneRegion,
  PlaneRegionKind,
  PlaneRender,
  StagedPlane,
  SummaryTileSlot,
} from './types';

/** The seats any attacker is currently attacking (crest ring at every rung). In
 * a duel the wire omits `attacking_player` (the sole opponent is the only
 * defender), so an attacking permanent implies the other player is attacked. */
function attackedSeats(
  view: GameView,
  receiver: PlayerId | undefined,
  duel: boolean,
): Set<PlayerId> {
  const attacked = new Set<PlayerId>();
  for (const perm of view.battlefield) {
    if (perm.attacking_player !== undefined) attacked.add(perm.attacking_player);
    else if (duel && perm.attacking === true) {
      const defender = perm.controller === receiver ? view.opponents[0]?.player_id : receiver;
      if (defender !== undefined) attacked.add(defender);
    }
  }
  return attacked;
}

/** Per-seat flags shared by regions and tiles. */
interface SeatFlags {
  eliminated: boolean;
  attacked: boolean;
  active: boolean;
  priority: boolean;
}

/**
 * Build the plane's staged scene from one view, a viewport, and the ephemeral
 * staging state — pure scene data (ADR 0030 layer 2), the successor of
 * `buildTableScene`'s band layout, implementing
 * `docs/design/layout-model.md`. WebGL/DOM-free and fully reconstructable from
 * a single view: the same inputs always stage the same plane.
 */
export function stagePlane(
  view: GameView,
  viewport: PlaneViewport,
  staging: PlaneStagingState = {},
): StagedPlane {
  const receiverSeat = localPlayerIdOf(view);
  const opponents = orderedOpponentIds(view);
  const battlefieldIds = new Set(view.battlefield.map((p) => p.id));
  const candidates = new Set((staging.candidates ?? []).filter((id) => battlefieldIds.has(id)));
  const subjectActions = view.valid_actions.filter((a) => a.subject && a.subject.length > 0);

  const byController = new Map<PlayerId, Permanent[]>();
  for (const perm of view.battlefield) {
    const list = byController.get(perm.controller) ?? [];
    list.push(perm);
    byController.set(perm.controller, list);
  }
  const itemsOf = (seat: PlayerId): StageItem[] =>
    buildStageItems(byController.get(seat) ?? [], subjectActions, candidates, staging.selectedId);

  const duel = opponents.length === 1;
  const focusSeat = resolveFocusSeat(view, opponents, staging.focusSeat, candidates);
  const farSeat = duel ? opponents[0] : focusSeat;
  const peripherals = opponents.filter((seat) => seat !== farSeat);
  const compact = isCompactGeometry(viewport) && opponents.length >= 2;

  const attacked = attackedSeats(view, receiverSeat, duel);
  const eliminated = new Set(view.opponents.filter((o) => o.eliminated).map((o) => o.player_id));
  const flagsOf = (seat: PlayerId): SeatFlags => ({
    eliminated: eliminated.has(seat),
    attacked: attacked.has(seat),
    active: view.active_player === seat,
    priority: view.priority_player === seat,
  });

  const slots =
    compact && farSeat !== undefined
      ? carveCompactSlots(viewport, peripherals)
      : carveSlots(viewport, receiverSeat !== undefined, farSeat, peripherals);

  const commander = hasCommandZone(view);
  // The stable seat list portraits and accents index into: server-authoritative,
  // never reordered, so a seat's face and colour hold for the whole game (§1.3).
  const seatOrder = seatOrderOf(view);

  const makeRegion = (
    seat: PlayerId,
    kind: PlaneRegionKind,
    rect: Rect,
    surface: SurfaceTier,
    wing?: WingSlotFrame,
  ): PlaneRegion => {
    const isReceiver = kind === 'receiver';
    const opponent = view.opponents.find((entry) => entry.player_id === seat);
    const zones = zoneCountsOf(view, seat, seat === receiverSeat);
    // The seat's zone rack claims its region's outer flank first; the board then
    // stages inboard of it, so cards and zones never contend for one rect.
    const rack = stageRack({
      seat,
      kind,
      side: wing?.side,
      rect,
      viewport,
      zones,
      commander,
      digestBaseline: wing?.digestBaseline ?? false,
      corridor: slots.corridor,
    });
    const content = stageRegionContent(
      seat,
      itemsOf(seat),
      rect,
      surface,
      kind,
      wing?.digestBaseline ?? false,
      rack.inset,
    );
    const flags = flagsOf(seat);
    const focused = !duel && kind === 'far';
    const label = bandLabel(view, seat, seat === receiverSeat);
    const life = isReceiver ? view.me.life : (opponent?.life ?? 0);
    const handCount = isReceiver ? view.my_hand.length : (opponent?.hand_size ?? 0);
    const variant: ClusterVariant =
      kind === 'receiver'
        ? 'local'
        : kind === 'far'
          ? 'focused'
          : rack.variant === 'digest'
            ? 'compact'
            : 'wing';
    const cluster = stageSeatCluster({
      seat,
      variant,
      anchor: clusterAnchor(variant, rect, rack, viewport),
      viewport,
      outboard: wing?.side ?? 'left',
      // The seat's own zone rack is a physical object at a fixed place; the
      // nameplate steps around it rather than through it (`cluster.ts`).
      keepOut: rack.bounds,
      facts: seatIdentityFacts(view, {
        seat,
        life,
        handCount,
        libraryCount: zones.library,
        local: isReceiver,
        eliminated: flags.eliminated,
        priority: flags.priority,
        active: flags.active,
        focused,
        attacked: flags.attacked,
        attackedCount: attackerCountOn(view, seat, receiverSeat, duel),
        commandPileHidden:
          rack.variant === 'digest' || !rack.slots.some((slot) => slot.zone === 'command'),
      }),
    });
    // An opponent's hand is a face-down fan beside their cluster — a seat
    // fixture at every rung (issue #533). The receiver's own hand stays a
    // screen-space shell region (ADR 0032 §7), so it stages no plane fan; the
    // two share the curve in `table/handFan.ts`, not a surface.
    const handFan = isReceiver
      ? undefined
      : stageSeatHandFan({
          seat,
          count: handCount,
          d: cluster.d,
          portrait: cluster.portrait,
          keepOut: rack.bounds,
          viewport,
        });
    return {
      seat,
      kind,
      side: wing?.side,
      rank: wing?.rank,
      rect,
      crest: cluster.hit,
      cluster,
      handFan,
      piles: rack.bounds,
      rack,
      zones,
      surface: content.surface,
      rung: content.rung,
      renders: content.renders,
      digest: content.digest,
      label,
      life,
      handCount,
      focused,
      ...flags,
    };
  };

  const receiver =
    receiverSeat !== undefined && slots.receiver !== undefined
      ? makeRegion(receiverSeat, 'receiver', slots.receiver, 'field')
      : undefined;
  const farSide =
    farSeat !== undefined && slots.far !== undefined
      ? makeRegion(farSeat, 'far', slots.far.rect, slots.far.surface)
      : undefined;
  const wings = slots.wings.map((wing) =>
    makeRegion(wing.seat, 'wing', wing.rect, wing.surface, wing),
  );

  // The tile column's growth budget: strips may spend only the slack between
  // the carved column and the receiver's band, so a grown tile can never push
  // the column into the receiver (the fixed slots stay non-overlapping).
  const lastTile = slots.tiles[slots.tiles.length - 1];
  let stripSlack =
    lastTile !== undefined && slots.receiver !== undefined
      ? Math.max(0, slots.receiver.y - (lastTile.rect.y + lastTile.rect.h))
      : 0;
  const tiles: SummaryTileSlot[] = slots.tiles.map(({ seat, rect }) => {
    const strip = tileCandidates(seat, itemsOf(seat), rect, stripSlack);
    stripSlack -= strip.rect.h - rect.h;
    const opponent = view.opponents.find((o) => o.player_id === seat);
    return {
      seat,
      rect: strip.rect,
      crest: { x: rect.x + 8, y: rect.y + (PLANE.compact.tile.h - 32) / 2, w: 32, h: 32 },
      portraitSrc: portraitFor(seat, seatOrder, false)?.src,
      accent: seatAccent(seatOrder, seat),
      label: bandLabel(view, seat, false),
      life: opponent?.life ?? 0,
      handCount: opponent?.hand_size ?? 0,
      zones: zoneCountsOf(view, seat, false),
      candidates: strip.candidates,
      candidateOverflow: strip.overflow,
      ...flagsOf(seat),
    };
  });
  // A grown tile pushes the ones below it down, keeping the column gap.
  for (let i = 1; i < tiles.length; i += 1) {
    const above = tiles[i - 1]!;
    const wanted = above.rect.y + above.rect.h + PLANE.compact.tile.gap;
    const tile = tiles[i]!;
    const dy = wanted - tile.rect.y;
    if (dy > 0) {
      tile.rect = { ...tile.rect, y: tile.rect.y + dy };
      tile.crest = { ...tile.crest, y: tile.crest.y + dy };
      for (const c of tile.candidates) {
        c.rect = { ...c.rect, y: c.rect.y + dy };
        c.hitRect = hitRectFor(c.rect);
      }
    }
  }

  return {
    width: viewport.width,
    height: viewport.height,
    compact,
    focusSeat,
    corridor: slots.corridor,
    receiver,
    farSide,
    wings,
    tiles,
    seats: [...(receiverSeat !== undefined ? [receiverSeat] : []), ...opponents],
  };
}

/**
 * Whether this game has a command zone at all (`zone-geography.md` §5, §12 gap
 * G3). `GameView` carries no format signal — `command`, `commander_tax`, and
 * `commander_damage` are each "omitted when empty" — so the client's only honest
 * test is whether any of them names anyone. The documented consequence is that a
 * Commander game whose commanders are all on the battlefield with no tax and no
 * damage yet reads as a non-Commander game and ships the slot **absent**, which
 * §5 prescribes over an unexplained empty box. Closing G3 (#553) is the fix; the
 * client never infers a format from anything else.
 */
function hasCommandZone(view: GameView): boolean {
  return (
    (view.command ?? []).length > 0 ||
    (view.commander_tax ?? []).length > 0 ||
    view.commander_damage.length > 0
  );
}

/**
 * Where a seat's portrait medallion centres — the anchor every offset in
 * `cluster.ts` is measured from.
 *
 * `seat-identity.md` §8 fixes the anchors from the environment sheet: **the
 * local cluster is always bottom centre**, straddling the receiver band's outer
 * edge; **the focused cluster is always top centre**, on its board's own top
 * edge; wings step down the ellipse, outboard of their board strip. The first
 * two are a deliberate move away from issue #531's shared rack origin, which the
 * baselines contradict — the receiver's medallion belongs above the hand, not in
 * the corner of its own board. `zone-geography.md` §2.2 still measures the rack
 * from "the identity medallion's centre", so for the receiver and the focused
 * seat the two anchors have separated; that conflict is reported, not edited.
 * Wing clusters keep the rack origin, where §8 and §2.2 already agree.
 *
 * Two guarantees ride on the clamp. The cluster is the player-targeting surface
 * and can never degrade away (layout-model §Staging), so it must be **on the
 * plane**; and it is drawn layout content, so it must stay inside the seat
 * envelope (`environment-system.md` §2.2/§3.3) — outside the focal core that
 * means inside the flank band, because everything else out there is Zone C: the
 * theme's prop pockets, the `AMBIENT SPACE` reservation, and the wordmark.
 */
function clusterAnchor(
  variant: ClusterVariant,
  slot: Rect,
  rack: SeatRack,
  viewport: PlaneViewport,
): { x: number; y: number } {
  const d = clusterD(variant, viewport);
  let cx: number;
  let cy: number;
  if (variant === 'local') {
    cx = slot.x + slot.w / 2;
    // Low enough to read as the receiver's own piece above the hand, high
    // enough that the life medallion and hand pip stay on the plane.
    cy = Math.min(slot.y + slot.h - 0.62 * d, viewport.height - 1.05 * d - 4);
  } else if (variant === 'focused') {
    cx = slot.x + slot.w / 2;
    cy = slot.y;
  } else if (rack.variant === 'digest') {
    // A digest rack is one button on the outer edge; the cluster sits along the
    // reading axis just past it rather than on top of it.
    cx = rack.bounds.x + d / 2;
    cy = rack.bounds.y + rack.bounds.h + 6 + d / 2;
  } else {
    cx = rack.origin.x;
    cy = rack.origin.y;
  }
  const clamped = clampToEnvelope(
    hitRectFor({ x: cx - d / 2, y: cy - d / 2, w: d, h: d }),
    viewport,
  );
  return { x: clamped.x + clamped.w / 2, y: clamped.y + clamped.h / 2 };
}

/** The strip's own height for `rows` candidate rows (0 when no rows fit). */
function stripHeight(rows: number): number {
  if (rows === 0) return 0;
  return (
    PLANE.compact.tile.stripGap + rows * CARD_BOX.mini.permanent.h + (rows - 1) * PLANE.rowGap + 8
  );
}

/**
 * A summary tile's candidate strip: prompt candidates stage individually inside
 * the tile, which grows below its header row to hold them — wrapping into rows
 * bounded by the tile's width, growing only inside the column's `slack` budget
 * (never past the receiver's band), so the strip can never spill into the
 * corridor or another slot. Candidates beyond the granted allocation are
 * reported as `overflow`: the tile's ≥ 44 px activation opens the
 * zone-browser-style pick surface listing every candidate (the carried
 * interaction guarantee — a pick is never removed, layout-model §Interaction
 * guarantees), so each remains addressable without a focus change.
 */
function tileCandidates(
  seat: PlayerId,
  items: StageItem[],
  rect: Rect,
  slack: number,
): { rect: Rect; candidates: PlaneRender[]; overflow: number } {
  const picks = items.filter((item) => item.candidate);
  if (picks.length === 0) return { rect, candidates: [], overflow: 0 };
  const innerW = rect.w - 16;
  const perRow = Math.max(
    1,
    Math.floor((innerW + PLANE.cardGap) / (CARD_BOX.mini.permanent.w + PLANE.cardGap)),
  );
  const rowsNeeded = Math.ceil(picks.length / perRow);
  let rows = 0;
  while (rows < rowsNeeded && stripHeight(rows + 1) <= slack) rows += 1;
  const staged = picks.slice(0, rows * perRow);
  const candidates: PlaneRender[] = staged.map((item, i) => {
    const row = Math.floor(i / perRow);
    const col = i % perRow;
    const r: Rect = {
      x: rect.x + 8 + col * (CARD_BOX.mini.permanent.w + PLANE.cardGap),
      y:
        rect.y +
        PLANE.compact.tile.h +
        PLANE.compact.tile.stripGap +
        row * (CARD_BOX.mini.permanent.h + PLANE.rowGap),
      w: CARD_BOX.mini.permanent.w,
      h: CARD_BOX.mini.permanent.h,
    };
    return {
      entityId: item.perm.id,
      seat,
      name: item.perm.card.name,
      row: item.row,
      tier: 'mini',
      rect: r,
      hitRect: hitRectFor(r),
      tapped: item.perm.tapped ?? false,
      memberIds: [item.perm.id],
      stackCount: 1,
      candidate: true,
      attacking: item.perm.attacking ?? false,
      blocking: item.perm.blocking !== undefined,
      attachedTo: item.perm.attached_to,
    };
  });
  const grown: Rect = { ...rect, h: PLANE.compact.tile.h + stripHeight(rows) };
  return { rect: grown, candidates, overflow: picks.length - staged.length };
}
