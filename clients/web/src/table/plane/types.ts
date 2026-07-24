import type { EntityId, PlayerId } from '../../protocol';
import type { RenderTier } from '../../card/cardFactory';
import type { Rect, SurfaceTier, BandRowKind, ZoneCounts } from '../scene/types';

/**
 * The viewport geometry the plane is staged for. The plane's logical coordinate
 * space equals the viewport; slot rects may extend slightly beyond it (the plane
 * bleeds past the visible stage — wings tuck partway offstage, crests ride above
 * the far edge), exactly like the staging prototype's felt.
 */
export interface PlaneViewport {
  /** Logical plane width. */
  width: number;
  /** Logical plane height. */
  height: number;
}

/**
 * The ephemeral presentation state `stagePlane` stages against, all of it
 * dropped and re-derived on the next view (one-view reconstruction, like
 * selection):
 *
 * - `focusSeat` — manual focus (layout-model §Focus model): the seat the player
 *   last activated via a wing crest, board, or summary tile. Honored only when
 *   it names a current opponent; otherwise the default-relevance focus applies.
 * - `candidates` — the active prompt's candidate entity ids, straight from the
 *   server's enumeration (the client computes no legality). Candidate objects
 *   pierce every rung: they always stage as individually addressable renders.
 * - `selectedId` — the current selection; a selected permanent never folds into
 *   an ×N pile.
 */
export interface PlaneStagingState {
  /** Manual (ephemeral) focus seat, if the player re-staged one. */
  focusSeat?: PlayerId;
  /** The active prompt's candidate entity ids (server-enumerated). */
  candidates?: EntityId[];
  /** The currently selected entity, if any. */
  selectedId?: EntityId;
}

/** Which fixed slot group a staged region occupies (layout-model §The plane). */
export type PlaneRegionKind = 'receiver' | 'far' | 'wing';

/** Which side of the plane a wing is staged on. */
export type WingSide = 'left' | 'right';

/**
 * The degradation-ladder rung a region resolved (layout-model §The degradation
 * ladder): 0 = full tier; 1 = tier step-down; 2 = ×N folding; 3 = row wrapping;
 * 4 = digest (wings only). Rung 5 (compact change-of-kind) is a plane-level
 * branch, reported by {@link StagedPlane.compact}, not a per-region rung.
 */
export type LadderRung = 0 | 1 | 2 | 3 | 4;

/**
 * One individually addressable object staged on the plane. Geometry only — the
 * renderer issues map `entityId` to display data; no CardDisplayData, no
 * legality. A render exists for every permanent the player might need to pick:
 * folding and digests never remove one (candidates, combat participants,
 * attachments, and the selection always force individual renders).
 */
export interface PlaneRender {
  /** Entity id (for an ×N pile, the representative — first member in server order). */
  entityId: EntityId;
  /** The seat whose region this render is staged in (the controller). */
  seat: PlayerId;
  /** Display name, for labels/aria. */
  name: string;
  /** The type-grouped row it lays in (carried sorting convention, not a zone). */
  row: BandRowKind;
  /** The card tier this render draws at. */
  tier: RenderTier;
  /** The visible footprint (rotated bounding box when tapped). */
  rect: Rect;
  /**
   * The interactive hotspot: `rect` grown to the 44 px floor in each dimension
   * (presentation-budgets §Accessibility) so every staged object stays a legal
   * touch target at every tier.
   */
  hitRect: Rect;
  /** Whether the permanent is tapped (drives the footprint). */
  tapped: boolean;
  /** Every permanent this render stands for; length 1 unless folded (×N). */
  memberIds: EntityId[];
  /** `memberIds.length` — the ×N badge count; 1 for a normal render. */
  stackCount: number;
  /** Whether this render is one of the active prompt's candidates (pierces rungs). */
  candidate: boolean;
  /** Whether the permanent is attacking (combat treatment; never folded). */
  attacking: boolean;
  /** Whether the permanent is blocking (combat treatment; never folded). */
  blocking: boolean;
  /** The host this permanent is attached to, when attached (never folded). */
  attachedTo?: EntityId;
}

/**
 * A digest-rung wing's summary counts (ladder rung 4): one count per
 * battlefield permanent category **present**, so a noncreature-heavy board can
 * never read as empty. Categories follow layout-model §The degradation ladder —
 * note planeswalkers/battles count as "other permanents" here even though the
 * row convention seats them with creatures. Pile counts ride
 * {@link PlaneRegion.zones}; prompt candidates pierce the digest and stage as
 * {@link PlaneRegion.renders}.
 */
export interface WingDigest {
  /** Creatures on the board, folded tokens counted individually. */
  creatures: number;
  /** Other permanents: artifacts, enchantments, planeswalkers, battles. */
  others: number;
  /** Lands. */
  lands: number;
}

/**
 * One seat's staged region on the plane: a fixed slot (receiver band, far side,
 * or wing) plus everything the seat keeps visible at every count and rung —
 * the crest cluster (the selection surface for player-targeting; it can never
 * degrade away) and the zone piles.
 */
export interface PlaneRegion {
  /** The seat this region belongs to. */
  seat: PlayerId;
  /** Which fixed slot group the region occupies. */
  kind: PlaneRegionKind;
  /** Wing side (wings only). */
  side?: WingSide;
  /** Wing row from the top, 0-based (wings only). */
  rank?: number;
  /** The region's slot rect — fixed by the stage; the ladder works inside it. */
  rect: Rect;
  /** The crest cluster's rect (≥ 44 px; always staged, every count and rung). */
  crest: Rect;
  /** The zone-pile cluster's rect (inside the slot, at the inner corner). */
  piles: Rect;
  /** The seat's pile counts, straight from the view. */
  zones: ZoneCounts;
  /** The surface tier the region resolved after the ladder. */
  surface: SurfaceTier;
  /** The degradation-ladder rung the region resolved, engaged independently. */
  rung: LadderRung;
  /** The individually addressable renders staged in the slot (candidates only,
   * at the digest rung). */
  renders: PlaneRender[];
  /** The digest counts (rung 4 only). */
  digest?: WingDigest;
  /** The seat's display label (controller name; "(you)"-marked for the receiver). */
  label: string;
  /** Whether the seat has been eliminated — the slot stays, with the eliminated
   * treatment (public zones remain browsable). */
  eliminated: boolean;
  /** Whether this seat is the focused opponent (far side at 3+ players). */
  focused: boolean;
  /** Whether any attacker is attacking this seat (crest ring, at every rung). */
  attacked: boolean;
  /** Whether this seat is the active player (turn owner). */
  active: boolean;
  /** Whether this seat holds priority. */
  priority: boolean;
}

/**
 * A compact change-of-kind summary tile (ladder rung 5, phone portrait at 3+
 * players): crest, life, hand/library counts, and the attacked/active markers,
 * with a ≥ 44 px activation rect that re-stages focus in place. A tile with
 * prompt candidates grows a candidate strip — the candidates stage as
 * individually addressable renders inside the tile rect, so answering a prompt
 * never requires a focus change.
 */
export interface SummaryTileSlot {
  /** The seat this tile summarizes. */
  seat: PlayerId;
  /** The tile's rect (≥ 44 px tall — the activation target). */
  rect: Rect;
  /** The mini-crest rect inside the tile. */
  crest: Rect;
  /** The seat's display label. */
  label: string;
  /** Life total, straight from the view. */
  life: number;
  /** Hand size, straight from the view. */
  handCount: number;
  /** The seat's pile counts. */
  zones: ZoneCounts;
  /** The candidate strip: prompt candidates staged individually in the tile. */
  candidates: PlaneRender[];
  /** Whether the seat has been eliminated. */
  eliminated: boolean;
  /** Whether any attacker is attacking this seat. */
  attacked: boolean;
  /** Whether this seat is the active player. */
  active: boolean;
  /** Whether this seat holds priority. */
  priority: boolean;
}

/**
 * The staged plane: pure scene data — fixed slots, per-region ladder outcomes,
 * and individually addressable renders — for one view + viewport + ephemeral
 * staging state. Consumed by the Phase 1 renderer issues; until then only the
 * fixture battlefield reads it.
 */
export interface StagedPlane {
  /** Logical plane width (equals the viewport's). */
  width: number;
  /** Logical plane height (equals the viewport's). */
  height: number;
  /**
   * Whether the compact change-of-kind engaged (ladder rung 5: phone-portrait
   * geometry with two or more opponents). When true, peripheral opponents stage
   * as {@link tiles}; a phone duel stays `false` and draws both boards in full.
   */
  compact: boolean;
  /** The focused opponent at 3+ players; absent at 2 (no focus concept). */
  focusSeat?: PlayerId;
  /**
   * The center corridor between the far side and the receiver's band — the
   * interaction area for targeting paths, combat webs, and temporary staging.
   * Kept clear by construction: no region, crest, pile, render, or tile
   * intersects it.
   */
  corridor: Rect;
  /** The receiver's band (bottom, largest tiers); absent when the receiver is
   * unknown (legacy/spectator view — every seat stages as an opponent). */
  receiver?: PlaneRegion;
  /** The far side: the focused opponent's expanded board (the sole opponent's,
   * full width, at 2 players). Absent only with no opponents at all. */
  farSide?: PlaneRegion;
  /** Peripheral opponents' wings, in stable seat order. */
  wings: PlaneRegion[];
  /** Compact summary tiles (rung 5 only), in stable seat order. */
  tiles: SummaryTileSlot[];
  /** Every staged seat in staging order: receiver first (when known), then
   * opponents in stable seat order. */
  seats: PlayerId[];
}
