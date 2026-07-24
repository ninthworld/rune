import type { GameView, Permanent, PlayerId } from '../../protocol';
import { TIER } from '../../tokens';
import type { Rect, SurfaceTier } from '../scene/types';
import {
  localPlayerIdOf,
  orderedOpponentIds,
  bandLabel,
  zoneCountsOf,
} from '../scene/band-helpers';
import { PLANE, isPhoneGeometry, hitRectFor } from './metrics';
import { carveSlots, carveCompactSlots, type WingSlotFrame } from './slots';
import { resolveFocusSeat } from './focus';
import { buildStageItems, stageRegionContent, type StageItem } from './regions';
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
  const compact = isPhoneGeometry(viewport) && opponents.length >= 2;

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

  const makeRegion = (
    seat: PlayerId,
    kind: PlaneRegionKind,
    rect: Rect,
    surface: SurfaceTier,
    wing?: WingSlotFrame,
  ): PlaneRegion => {
    const isReceiver = kind === 'receiver';
    const content = stageRegionContent(
      seat,
      itemsOf(seat),
      rect,
      surface,
      kind === 'wing',
      wing?.digestBaseline ?? false,
    );
    // The crest cluster: beside the receiver's band, centered above any
    // opponent region — always present, at every count and every rung.
    const crest: Rect = isReceiver
      ? {
          x: Math.max(4, rect.x - PLANE.crest.w - 64),
          y: rect.y + 30,
          w: PLANE.crest.w,
          h: PLANE.crest.h,
        }
      : {
          x: rect.x + rect.w / 2 - PLANE.crest.w / 2,
          y: rect.y - PLANE.crest.h - 6,
          w: PLANE.crest.w,
          h: PLANE.crest.h,
        };
    return {
      seat,
      kind,
      side: wing?.side,
      rank: wing?.rank,
      rect,
      crest: hitRectFor(crest),
      piles: {
        x: rect.x + rect.w - PLANE.pile.w - 4,
        y: rect.y + rect.h - PLANE.pile.h - 4,
        w: PLANE.pile.w,
        h: PLANE.pile.h,
      },
      zones: zoneCountsOf(view, seat, seat === receiverSeat),
      surface: content.surface,
      rung: content.rung,
      renders: content.renders,
      digest: content.digest,
      label: bandLabel(view, seat, seat === receiverSeat),
      focused: !duel && kind === 'far',
      ...flagsOf(seat),
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

  const tiles: SummaryTileSlot[] = slots.tiles.map(({ seat, rect }) => {
    const strip = tileCandidates(seat, itemsOf(seat), rect);
    const opponent = view.opponents.find((o) => o.player_id === seat);
    return {
      seat,
      rect: strip.rect,
      crest: { x: rect.x + 8, y: rect.y + (PLANE.compact.tile.h - 32) / 2, w: 32, h: 32 },
      label: bandLabel(view, seat, false),
      life: opponent?.life ?? 0,
      handCount: opponent?.hand_size ?? 0,
      zones: zoneCountsOf(view, seat, false),
      candidates: strip.candidates,
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
 * A summary tile's candidate strip: prompt candidates stage individually inside
 * the tile, which grows below its header row to hold them — so the candidates
 * stay pickable in place and the corridor beside the tile column stays clear.
 */
function tileCandidates(
  seat: PlayerId,
  items: StageItem[],
  rect: Rect,
): { rect: Rect; candidates: PlaneRender[] } {
  const picks = items.filter((item) => item.candidate);
  if (picks.length === 0) return { rect, candidates: [] };
  const gap = PLANE.compact.tile.stripGap;
  const candidates: PlaneRender[] = picks.map((item, i) => {
    const size = { w: TIER.mini.w, h: TIER.mini.h };
    const r: Rect = {
      x: rect.x + 8 + i * (size.w + PLANE.cardGap),
      y: rect.y + PLANE.compact.tile.h + gap,
      w: size.w,
      h: size.h,
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
  const grown: Rect = { ...rect, h: PLANE.compact.tile.h + gap + TIER.mini.h + 8 };
  return { rect: grown, candidates };
}
