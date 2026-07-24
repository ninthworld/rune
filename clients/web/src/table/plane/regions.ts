import type { EntityId, Permanent, PlayerId, ValidAction } from '../../protocol';
import { cardVisualSignature, type RenderTier } from '../../card/cardFactory';
import type { Rect, SurfaceTier, BandRowKind } from '../scene/types';
import { tiersForSurface, stepDown, actionsFor } from '../scene/action-helpers';
import {
  rowKindForType,
  actionFingerprint,
  toDisplayData,
  basicLandGlyph,
} from '../scene/card-helpers';
import { cellSize } from '../scene/geometry';
import { PLANE, insetRect, hitRectFor } from './metrics';
import type { LadderRung, PlaneRender, WingDigest } from './types';

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

/** The order the type-grouped rows stack, top to bottom (carried convention). */
const ROW_ORDER: BandRowKind[] = ['creatures', 'support', 'lands'];

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

/**
 * Lay groups into the content area: type-grouped rows top-to-bottom, each row's
 * cards on centered lines (bottom-aligned within a line). Without `wrap` every
 * row is a single line and overflow is reported via `maxLineWidth`; with `wrap`
 * (ladder rung 3) lines break inside the slot's width — the slot's height stays
 * fixed by the stage, so wrapping trades row height, never neighbor space.
 */
function layGroups(
  groups: StageGroup[],
  seat: PlayerId,
  tiers: Record<BandRowKind, RenderTier>,
  content: Rect,
  wrap: boolean,
): LayResult {
  const renders: PlaneRender[] = [];
  let y = content.y;
  let maxLineWidth = 0;
  let any = false;
  for (const row of ROW_ORDER) {
    const rowGroups = groups.filter((g) => g.item.row === row);
    if (rowGroups.length === 0) continue;
    any = true;
    const tier = tiers[row];
    const cells = rowGroups.map((g) => ({
      g,
      size: cellSize(tier, g.item.perm.tapped ?? false),
    }));
    // Break into lines: one line unless wrapping past the content width.
    const lines: (typeof cells)[] = [];
    let line: typeof cells = [];
    let lineW = 0;
    for (const cell of cells) {
      const next = lineW === 0 ? cell.size.w : lineW + PLANE.cardGap + cell.size.w;
      if (wrap && line.length > 0 && next > content.w) {
        lines.push(line);
        line = [cell];
        lineW = cell.size.w;
      } else {
        line.push(cell);
        lineW = next;
      }
    }
    if (line.length > 0) lines.push(line);
    for (const cells2 of lines) {
      const width =
        cells2.reduce((sum, c) => sum + c.size.w, 0) + (cells2.length - 1) * PLANE.cardGap;
      const lineH = Math.max(...cells2.map((c) => c.size.h));
      maxLineWidth = Math.max(maxLineWidth, width);
      let x = content.x + Math.max(0, Math.floor((content.w - width) / 2));
      for (const cell of cells2) {
        const rect: Rect = { x, y: y + lineH - cell.size.h, w: cell.size.w, h: cell.size.h };
        renders.push(toRender(cell.g, seat, tiers[cell.g.item.row], rect));
        x += cell.size.w + PLANE.cardGap;
      }
      y += lineH + PLANE.rowGap;
    }
  }
  return { renders, height: any ? y - PLANE.rowGap - content.y : 0, maxLineWidth };
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
  const laid = layGroups(candidates, seat, tiers, content, true);
  shiftRenders(laid.renders, Math.max(0, Math.floor((content.h - laid.height) / 2)));
  return { renders: laid.renders, rung: 4, surface, digest };
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
  wing: boolean,
  digestBaseline: boolean,
): RegionContent {
  const content = insetRect(slot, PLANE.pad);
  if (wing && digestBaseline) return digestStage(seat, items, content, baseSurface);

  const stepped = stepDown(baseSurface);
  const attempts: { rung: LadderRung; surface: SurfaceTier; fold: boolean; wrap: boolean }[] = [
    { rung: 0, surface: baseSurface, fold: false, wrap: false },
    ...(stepped !== baseSurface
      ? [{ rung: 1 as LadderRung, surface: stepped, fold: false, wrap: false }]
      : []),
    { rung: 2, surface: stepped, fold: true, wrap: false },
    { rung: 3, surface: stepped, fold: true, wrap: true },
  ];

  let last: { laid: LayResult; surface: SurfaceTier } | undefined;
  for (const attempt of attempts) {
    const groups = groupItems(items, attempt.fold);
    const laid = layGroups(groups, seat, tiersForSurface(attempt.surface), content, attempt.wrap);
    last = { laid, surface: attempt.surface };
    const fits = laid.height <= content.h && (attempt.wrap || laid.maxLineWidth <= content.w);
    if (fits) {
      shiftRenders(laid.renders, Math.max(0, Math.floor((content.h - laid.height) / 2)));
      return { renders: laid.renders, rung: attempt.rung, surface: attempt.surface };
    }
  }

  // Nothing fit at rung 3. A wing steps to its digest; the receiver and the far
  // side never digest — they compress vertically inside the fixed slot.
  if (wing) return digestStage(seat, items, content, last?.surface ?? stepped);
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
