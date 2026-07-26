import type { EntityId, Permanent, PlayerId, ValidAction } from '../../protocol';
import { cardVisualSignature, type RenderTier } from '../../card/cardFactory';
import type { Rect, SurfaceTier, BandRowKind } from '../scene/types';
import { tiersForSurface, stepDown, actionsFor } from '../scene/action-helpers';
import {
  rowKindForType,
  actionFingerprint,
  toDisplayData,
  basicLandGlyph,
  landRenderTier,
} from '../scene/card-helpers';
import { cellSize, splayClearance, surfaceKindForRow, tabClearance } from '../scene/geometry';
import { PLANE, insetRect, hitRectFor } from './metrics';
import type { LadderRung, PlaneRegionKind, PlaneRender, WingDigest } from './types';

/** One permanent prepared for staging: its row, fold key parts, and pick flags. */
export interface StageItem {
  /** The permanent, straight from the view. */
  perm: Permanent;
  /** The type-grouped row it sorts into (carried convention). */
  row: BandRowKind;
  /** The offered-action fingerprint — part of the carried ×N grouping key. */
  fingerprint: string;
  /** Whether the active prompt lists it as a candidate (pierces every rung). */
  candidate: boolean;
  /**
   * Whether it must render individually (never folds): a prompt candidate, the
   * current selection, a combat participant, or an attachment-cluster member.
   */
  forced: boolean;
}

/**
 * Prepare a seat's permanents for staging. `subjectActions` is the pre-filtered
 * entity-subject slice of `valid_actions` (interactivity derives from nothing
 * else); `candidates`/`selectedId` are the ephemeral staging state.
 */
export function buildStageItems(
  perms: Permanent[],
  subjectActions: ValidAction[],
  candidates: Set<EntityId>,
  selectedId: EntityId | undefined,
): StageItem[] {
  const hosts = new Set(perms.map((p) => p.attached_to).filter((id) => id !== undefined));
  return perms.map((perm) => {
    const candidate = candidates.has(perm.id);
    return {
      perm,
      row: rowKindForType(perm.card.type_line),
      fingerprint: actionFingerprint(actionsFor(perm.id, subjectActions)),
      candidate,
      forced:
        candidate ||
        perm.id === selectedId ||
        perm.attacking === true ||
        perm.blocking !== undefined ||
        perm.attached_to !== undefined ||
        hosts.has(perm.id),
    };
  });
}

/** An ×N group: the representative item plus every member it stands for. */
interface StageGroup {
  item: StageItem;
  memberIds: EntityId[];
}

/**
 * The carried ×N grouping key, shared byte-for-byte with the shipped client's
 * `groupStacks`: the card's **full visual signature** (`cardVisualSignature`,
 * which covers every renderer-visible input — type line, P/T, counters, tap,
 * keywords, damage, the `rules_text`-derived ability marker, and the
 * `functional_id`-derived art key) plus the offered-action fingerprint. Two
 * permanents fold only when a player could not tell their renders apart. Forced
 * items never reach this, so the interactive flags are their foldable defaults.
 */
function foldKey(item: StageItem): string {
  const { perm } = item;
  const data = toDisplayData(perm.card, {
    tapped: perm.tapped,
    counters: perm.counters,
    selected: false,
    actionable: item.fingerprint !== '',
    landGlyph: item.row === 'lands' ? basicLandGlyph(perm.card.type_line) : undefined,
    // The land **resource tile** silhouette is part of what a player sees, so it
    // is part of the fold key: a 1.45 tile and a 1.00 square plaque at the same
    // tier are different objects and must never fold into one ×N pile (issue
    // #529, card-representation §3.1/§4).
    landTile: item.row === 'lands',
    attacking: perm.attacking,
    attackingPlayer: perm.attacking_player,
    blocking: perm.blocking !== undefined,
    markedDamage: perm.damage,
  });
  return `${cardVisualSignature(data)}|${item.fingerprint}`;
}

/**
 * Fold identical-full-state permanents into ×N groups (ladder rung 2) under the
 * carried {@link foldKey} — a stack never hides a differing card, and a forced
 * item (candidate, selection, combat participant, attachment) always stays its
 * own individually addressable group.
 */
function groupItems(items: StageItem[], fold: boolean): StageGroup[] {
  if (!fold) return items.map((item) => ({ item, memberIds: [item.perm.id] }));
  const groups: StageGroup[] = [];
  const at = new Map<string, number>();
  for (const item of items) {
    if (item.forced) {
      groups.push({ item, memberIds: [item.perm.id] });
      continue;
    }
    const key = foldKey(item);
    const index = at.get(key);
    if (index === undefined) {
      at.set(key, groups.length);
      groups.push({ item, memberIds: [item.perm.id] });
    } else {
      groups[index]!.memberIds.push(item.perm.id);
    }
  }
  return groups;
}

/**
 * The order the type-grouped rows stack, top to bottom.
 *
 * The baseline arena reads rows **outward from their owner** (issue #531,
 * `rune-2.5d-interface-baseline.jpg`): the row nearest the seat's own edge of
 * the plane is its lands/resources, and the row nearest the open centre is its
 * creatures, so every seat's board faces the arena the same way and ownership
 * comes from orientation rather than from a labelled panel. For the receiver at
 * the bottom and for a flank wing that is the carried top-to-bottom order; for
 * the focused opponent across the top it is the reverse, because that seat's own
 * edge is the top of the plane.
 */
const ROW_ORDER: BandRowKind[] = ['creatures', 'support', 'lands'];

/** The row order for one region kind — reversed only for the far side. */
function rowOrderFor(kind: PlaneRegionKind): BandRowKind[] {
  return kind === 'far' ? [...ROW_ORDER].reverse() : ROW_ORDER;
}

interface LayResult {
  renders: PlaneRender[];
  height: number;
  maxLineWidth: number;
}

/** Build the render for one group at its final rect. */
function toRender(group: StageGroup, seat: PlayerId, tier: RenderTier, rect: Rect): PlaneRender {
  const { perm, row, candidate } = group.item;
  return {
    entityId: perm.id,
    seat,
    name: perm.card.name,
    row,
    tier,
    rect,
    hitRect: hitRectFor(rect),
    tapped: perm.tapped ?? false,
    memberIds: group.memberIds,
    stackCount: group.memberIds.length,
    candidate,
    attacking: perm.attacking ?? false,
    blocking: perm.blocking !== undefined,
    attachedTo: perm.attached_to,
  };
}

/** One cell about to be laid: its group, its reserved box, and the clearances
 * that box's decorations sweep outside it (issue #529). */
interface Cell {
  g: StageGroup;
  /** The tier this cell draws at — its row's rung, or the land promotion. */
  tier: RenderTier;
  size: { w: number; h: number };
  /** Clearance the `×N` top-edge tab needs above the box (0 when unfolded). */
  tab: number;
  /** Clearance the down-and-left splay sweeps (0 when unfolded). */
  splay: { left: number; down: number };
}

/**
 * The tier one staged permanent draws at: its row's rung, except that a land
 * takes {@link landRenderTier}'s promotion off the chip rung when it carries no
 * basic emblem (issue #463 — a nonbasic land is never an anonymous chip).
 */
function tierForItem(item: StageItem, tiers: Record<BandRowKind, RenderTier>): RenderTier {
  const rowTier = tiers[item.row];
  return item.row === 'lands' ? landRenderTier(item.perm.card.type_line, rowTier) : rowTier;
}

/** Reserve one group's cell, including whatever its fold decorations overhang. */
function toCell(group: StageGroup, tier: RenderTier): Cell {
  const { item } = group;
  const kind = surfaceKindForRow(tier, item.row === 'lands');
  const folded = group.memberIds.length > 1;
  return {
    g: group,
    tier,
    size: cellSize(tier, item.perm.tapped ?? false, kind),
    // The count tab and the splay are drawn by a fold and by nothing else, so an
    // unfolded card reserves exactly its own box — the ladder is not paying for
    // decorations that are not there.
    tab: folded ? tabClearance(tier, kind) : 0,
    splay: folded ? splayClearance(tier, kind) : { left: 0, down: 0 },
  };
}

/**
 * Lay groups into the content area: type-grouped rows in {@link rowOrderFor}'s
 * order, each row's cards on centered lines (bottom-aligned within a line).
 * Without `wrap` every row is a single line and overflow is reported via
 * `maxLineWidth`; with `wrap` (ladder rung 3) lines break inside the slot's
 * width — the slot's height stays fixed by the stage, so wrapping trades row
 * height, never neighbor space.
 *
 * Every line reserves the clearances a **folded** pile sweeps outside its own
 * box (issue #529): the `×N` count is a top-edge tab overhanging the card by
 * half its own height, and the pile splays **down-and-left**. Without the
 * reservation a fold's count would collide with the row above it and its depth
 * would underlap the neighbour to its left.
 */
function layGroups(
  groups: StageGroup[],
  seat: PlayerId,
  tiers: Record<BandRowKind, RenderTier>,
  content: Rect,
  wrap: boolean,
  order: BandRowKind[],
): LayResult {
  const renders: PlaneRender[] = [];
  let y = content.y;
  let maxLineWidth = 0;
  let any = false;
  for (const row of order) {
    const rowGroups = groups.filter((g) => g.item.row === row);
    if (rowGroups.length === 0) continue;
    any = true;
    const cells = rowGroups.map((g) => toCell(g, tierForItem(g.item, tiers)));
    // Break into lines: one line unless wrapping past the content width. A
    // folded cell's leftward splay is charged to the gap ahead of it.
    const lines: Cell[][] = [];
    let line: Cell[] = [];
    let lineW = 0;
    for (const cell of cells) {
      const lead = lineW === 0 ? cell.splay.left : PLANE.cardGap + cell.splay.left;
      const next = lineW + lead + cell.size.w;
      if (wrap && line.length > 0 && next > content.w) {
        lines.push(line);
        line = [cell];
        lineW = cell.splay.left + cell.size.w;
      } else {
        line.push(cell);
        lineW = next;
      }
    }
    if (line.length > 0) lines.push(line);
    for (const lineCells of lines) {
      const width = lineWidth(lineCells);
      const lineH = Math.max(...lineCells.map((c) => c.size.h));
      const tab = Math.max(...lineCells.map((c) => c.tab));
      const splayDown = Math.max(...lineCells.map((c) => c.splay.down));
      maxLineWidth = Math.max(maxLineWidth, width);
      let x = content.x + Math.max(0, Math.floor((content.w - width) / 2));
      for (const cell of lineCells) {
        x += cell.splay.left;
        const rect: Rect = { x, y: y + tab + lineH - cell.size.h, w: cell.size.w, h: cell.size.h };
        renders.push(toRender(cell.g, seat, cell.tier, rect));
        x += cell.size.w + PLANE.cardGap;
      }
      y += tab + lineH + splayDown + PLANE.rowGap;
    }
  }
  return { renders, height: any ? y - PLANE.rowGap - content.y : 0, maxLineWidth };
}

/** The horizontal extent one laid line occupies, splay overhang included. */
function lineWidth(cells: Cell[]): number {
  return cells.reduce(
    (sum, cell, i) => sum + cell.splay.left + cell.size.w + (i > 0 ? PLANE.cardGap : 0),
    0,
  );
}

/** Shift every render down by `dy` (vertical centering inside the fixed slot). */
function shiftRenders(renders: PlaneRender[], dy: number): void {
  for (const r of renders) {
    r.rect = { ...r.rect, y: r.rect.y + dy };
    r.hitRect = hitRectFor(r.rect);
  }
}

/** The digest category of a permanent (layout-model rung 4 — distinct from the
 * row convention: planeswalkers/battles count as "other permanents" here). */
function digestCategory(typeLine: string): keyof WingDigest {
  if (/\bCreature\b/.test(typeLine)) return 'creatures';
  if (/\bLand\b/.test(typeLine)) return 'lands';
  return 'others';
}

/** The staged content of one region: renders, the resolved rung, and a digest. */
export interface RegionContent {
  renders: PlaneRender[];
  rung: LadderRung;
  surface: SurfaceTier;
  digest?: WingDigest;
}

/** Stage a wing at the digest rung: all-category counts, candidates piercing. */
function digestStage(
  seat: PlayerId,
  items: StageItem[],
  content: Rect,
  surface: SurfaceTier,
  order: BandRowKind[],
): RegionContent {
  const digest: WingDigest = { creatures: 0, others: 0, lands: 0 };
  for (const item of items) digest[digestCategory(item.perm.card.type_line)] += 1;
  // Candidates pierce the rung: they render individually, centered in the slot,
  // at a tier that keeps them readable and pickable over the digest chips.
  const candidates = items
    .filter((item) => item.candidate)
    .map((item) => ({ item, memberIds: [item.perm.id] }));
  const tiers: Record<BandRowKind, RenderTier> = {
    creatures: 'mini',
    support: 'mini',
    lands: 'mini',
  };
  const laid = layGroups(candidates, seat, tiers, content, true, order);
  shiftRenders(laid.renders, Math.max(0, Math.floor((content.h - laid.height) / 2)));
  return { renders: laid.renders, rung: 4, surface, digest };
}

/** How far a board steps off each edge of its slot for the seat's own fixtures. */
interface PlaneInset {
  top: number;
  right: number;
  bottom: number;
  left: number;
}

/** No inset at all — the board gets its whole padded slot. */
const NO_INSET: PlaneInset = { top: 0, right: 0, bottom: 0, left: 0 };

/**
 * How far the board must step off an edge of its own content area to clear the
 * seat's identity cluster (issue #582 §1).
 *
 * `PLANE.crest` — a 52 × 52 "headroom constant a slot must clear above its
 * board" — was the reservation this was *supposed* to be, and it had two
 * problems: the drawn medallion is `cluster`'s rung `D`, which is 112 px local
 * and 96 px focused and therefore always larger than the constant, and nothing
 * in the staging path ever read the constant at all. The result is what the
 * maintainer's capture shows — the local medallion, life ring, and hand-count
 * hex drawn over the player's own creatures, and the opponent's medallion over
 * theirs. The reservation is derived from the cluster that is actually staged
 * now, so it cannot be smaller than the thing it reserves for.
 *
 * `core` is the cluster's medallion group (`cluster.ts`), not everything it
 * draws: that group is fixed at the anchor and cannot move, while the nameplate
 * steps around keep-outs and the status rail arcs around the portrait. Charging
 * a board for a plate that reaches two `D` to one side would cost it a whole row
 * for an element that is free to be somewhere else.
 *
 * The board steps off **one** edge, by whichever axis costs it least. That is
 * not a tie-breaker, it is the geometry: `seat-identity.md` §8 anchors the local
 * cluster on its band's bottom edge and the focused one on its top, so those two
 * are cheapest vertically and the row simply starts lower or ends higher; a
 * wing's cluster sits in the flank beside its board, so that one is cheapest
 * horizontally and the row simply starts further inboard, exactly as it already
 * does for the rack.
 *
 * A cluster whose core does not overlap the board's rect at all costs nothing —
 * which is the ordinary case for a wing, whose medallion sits in the flank the
 * rack inset already gave up.
 */
function clusterReserve(core: Rect, content: Rect): PlaneInset {
  const x0 = Math.max(core.x, content.x);
  const x1 = Math.min(core.x + core.w, content.x + content.w);
  const y0 = Math.max(core.y, content.y);
  const y1 = Math.min(core.y + core.h, content.y + content.h);
  if (x1 <= x0 || y1 <= y0) return NO_INSET;
  const left = x1 - content.x;
  const right = content.x + content.w - x0;
  const top = y1 - content.y;
  const bottom = content.y + content.h - y0;
  if (Math.min(top, bottom) <= Math.min(left, right)) {
    return top <= bottom ? { ...NO_INSET, top } : { ...NO_INSET, bottom };
  }
  return left <= right ? { ...NO_INSET, left } : { ...NO_INSET, right };
}

/**
 * Stage one region's permanents inside its fixed slot, engaging the degradation
 * ladder per region, independently (one hoarding player never shrinks another):
 * rung 0 full tier → 1 tier step-down → 2 ×N folding → 3 row wrapping → 4
 * digest (wings only; the far side and the receiver never digest — they
 * compress vertically inside their slot instead).
 */
export function stageRegionContent(
  seat: PlayerId,
  items: StageItem[],
  slot: Rect,
  baseSurface: SurfaceTier,
  kind: PlaneRegionKind,
  digestBaseline: boolean,
  rackInset: { left: number; right: number } = { left: 0, right: 0 },
  clusterCore?: Rect,
): RegionContent {
  const wing = kind === 'wing';
  const order = rowOrderFor(kind);
  const padded = insetRect(slot, PLANE.pad);
  // The seat's zone rack owns its region's outer flank (zone-geography §2.4), so
  // the board's content area starts inboard of it: the rack and the cards never
  // contend for the same pixels, at any rung.
  const flank: Rect = {
    x: padded.x + rackInset.left,
    y: padded.y,
    w: Math.max(0, padded.w - rackInset.left - rackInset.right),
    h: padded.h,
  };
  // …and the seat's identity cluster owns the band it is staged on, for the
  // same reason and by the same rule: a seat fixture is a physical object at a
  // fixed place, and the board lays out around it rather than under it.
  const fixture = clusterCore === undefined ? NO_INSET : clusterReserve(clusterCore, flank);
  const content: Rect = {
    x: flank.x + fixture.left,
    y: flank.y + fixture.top,
    w: Math.max(0, flank.w - fixture.left - fixture.right),
    h: Math.max(0, flank.h - fixture.top - fixture.bottom),
  };
  if (wing && digestBaseline) return digestStage(seat, items, content, baseSurface, order);

  const stepped = stepDown(baseSurface);
  const attempts: { rung: LadderRung; surface: SurfaceTier; fold: boolean; wrap: boolean }[] = [
    { rung: 0, surface: baseSurface, fold: false, wrap: false },
    ...(stepped !== baseSurface
      ? [{ rung: 1 as LadderRung, surface: stepped, fold: false, wrap: false }]
      : []),
    { rung: 2, surface: stepped, fold: true, wrap: false },
    { rung: 3, surface: stepped, fold: true, wrap: true },
    // A second tier step at the wrapping rung (issue #582 §1). Now that a board
    // gives up the band its own seat cluster stands on, a slot can be shorter
    // than one card at the stepped tier — and the compression fallback below
    // moves line STARTS without changing card heights, so a card taller than the
    // content area would still be drawn over the medallion the reservation
    // exists to clear. Stepping the tier again is the ladder's own answer to
    // "the board does not fit"; the compression that follows is for the case
    // where even the smallest tier does not.
    ...(stepDown(stepped) !== stepped
      ? [{ rung: 3 as LadderRung, surface: stepDown(stepped), fold: true, wrap: true }]
      : []),
  ];

  let last: { laid: LayResult; surface: SurfaceTier } | undefined;
  for (const attempt of attempts) {
    const groups = groupItems(items, attempt.fold);
    const laid = layGroups(
      groups,
      seat,
      tiersForSurface(attempt.surface),
      content,
      attempt.wrap,
      order,
    );
    last = { laid, surface: attempt.surface };
    const fits = laid.height <= content.h && (attempt.wrap || laid.maxLineWidth <= content.w);
    if (fits) {
      shiftRenders(laid.renders, Math.max(0, Math.floor((content.h - laid.height) / 2)));
      return { renders: laid.renders, rung: attempt.rung, surface: attempt.surface };
    }
  }

  // Nothing fit at rung 3. A wing steps to its digest; the receiver and the far
  // side never digest — they compress vertically inside the fixed slot.
  if (wing) return digestStage(seat, items, content, last?.surface ?? stepped, order);
  const laid = last?.laid ?? { renders: [], height: 0, maxLineWidth: 0 };
  if (laid.height > content.h && laid.renders.length > 0) {
    // Compress line starts so the block's travel fits above the tallest card's
    // own height (the carried squeeze: rows overlap, cards keep their size).
    const tallest = Math.max(...laid.renders.map((r) => r.rect.h));
    const travel = laid.height - tallest;
    const factor = travel > 0 ? Math.max(0.35, (content.h - tallest) / travel) : 1;
    for (const r of laid.renders) {
      r.rect = { ...r.rect, y: content.y + Math.round((r.rect.y - content.y) * factor) };
      r.hitRect = hitRectFor(r.rect);
    }
  }
  return { renders: laid.renders, rung: 3, surface: last?.surface ?? stepped };
}
