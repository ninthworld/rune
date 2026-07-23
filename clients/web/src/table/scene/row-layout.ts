import type { EntityId } from '../../protocol';
import { TIER } from '../../tokens';
import { M, cellSize } from './geometry';
import type { Rect, RenderedCard, BandRow, BandRowKind } from './types';
import { tiersForSurface, stepDown } from './action-helpers';

/**
 * Flow a row of (possibly mixed-footprint) cards inside a content area, wrapping
 * to a new line when the next card would cross the right edge, then **centering
 * each line** within the span — the blueprint mocks' centered row lines. Returns
 * the placed cards and the total height. Each card's `rect` is its **visible
 * footprint** (the rotated bounding box for a tapped card), so both the
 * reconciler's placement and the DOM hotspot cover the drawn card.
 */
export function flowRow(
  cards: Omit<RenderedCard, 'rect'>[],
  left: number,
  top: number,
  availWidth: number,
): { placed: RenderedCard[]; height: number } {
  if (cards.length === 0) return { placed: [], height: 0 };
  const limit = left + availWidth;
  const placed: RenderedCard[] = [];
  let x = left;
  let y = top;
  let lineHeight = 0;
  let lineStart = 0;

  const closeLine = (from: number): void => {
    if (from >= placed.length) return;
    const last = placed[placed.length - 1]!;
    const lineRight = last.rect.x + last.rect.w;
    const shift = Math.floor(Math.max(0, limit - lineRight) / 2);
    for (let i = from; i < placed.length; i += 1) {
      const rect = placed[i]!.rect;
      rect.x += shift;
      rect.y += lineHeight - rect.h;
    }
  };

  for (const card of cards) {
    const size = cellSize(card.tier, card.data.tapped ?? false);
    if (x !== left && x + size.w > limit) {
      closeLine(lineStart);
      lineStart = placed.length;
      x = left;
      y += lineHeight + M.rowGap;
      lineHeight = 0;
    }
    placed.push({ ...card, rect: { x, y, w: size.w, h: size.h } });
    x += size.w + M.cardGap;
    lineHeight = Math.max(lineHeight, size.h);
  }
  closeLine(lineStart);
  return { placed, height: y - top + lineHeight };
}

/** Per-panel layout result before finalization. */
interface PanelLayout {
  cards: RenderedCard[];
  rows: BandRow[];
  densityRung: number;
}

/**
 * Lay one player's permanents into their panel content area, engaging the density
 * ladder: full surface tier → one tier step down → vertical compression. Rows run
 * creatures / support / lands top-to-bottom (the shared mock vocabulary) and the
 * row block centers vertically in the content area.
 */
export function layPanel(
  renderables: Record<BandRowKind, Omit<RenderedCard, 'rect'>[]>,
  content: Rect,
  surface: import('./types').SurfaceTier,
): PanelLayout {
  const attempt = (
    tier: import('./types').SurfaceTier,
  ): { cards: RenderedCard[]; rows: BandRow[]; height: number } => {
    const tiers = tiersForSurface(tier);
    const rows: BandRow[] = [];
    const cards: RenderedCard[] = [];
    let top = 0;
    for (const kind of ['creatures', 'support', 'lands'] as BandRowKind[]) {
      const rowCards = renderables[kind].map((card) => ({ ...card, tier: tiers[kind] }));
      if (rowCards.length === 0) continue;
      const { placed, height } = flowRow(rowCards, content.x, content.y + top, content.w);
      cards.push(...placed);
      rows.push({
        kind,
        tier: tiers[kind],
        rect: { x: content.x, y: content.y + top, w: content.w, h: height },
        label: kind === 'lands' ? 'Lands' : undefined,
      });
      top += height + M.rowGap;
    }
    return { cards, rows, height: rows.length > 0 ? top - M.rowGap : 0 };
  };

  let rung = 0;
  let laid = attempt(surface);
  if (laid.height > content.h && surface !== 'mini') {
    rung = 1;
    laid = attempt(stepDown(surface));
  }

  let shift = Math.max(0, Math.floor((content.h - laid.height) / 2));
  let squeeze = 1;
  if (laid.height > content.h && laid.height > 0) {
    rung = 2;
    shift = 0;
    const lastRow = laid.rows[laid.rows.length - 1]!;
    const lastRowH = lastRow.rect.h;
    const travel = laid.height - lastRowH;
    if (travel > 0) squeeze = Math.max(0.35, (content.h - lastRowH) / travel);
  }
  const remap = (y: number): number => content.y + Math.round((y - content.y) * squeeze) + shift;
  for (const card of laid.cards) card.rect.y = remap(card.rect.y);
  for (const row of laid.rows) row.rect.y = remap(row.rect.y);

  return { cards: laid.cards, rows: laid.rows, densityRung: rung };
}

/**
 * Lay the hand as one centered row that compresses into an overlapping fan when
 * it outgrows its area (never wrapping — the hand row is a fixed shell home). A
 * selected card lifts by {@link M.handLift}; later cards draw over earlier ones,
 * matching the fan's physical stacking.
 */
export function layHand(
  cards: Omit<RenderedCard, 'rect'>[],
  area: Rect,
  selectedId: EntityId | undefined,
): RenderedCard[] {
  if (cards.length === 0) return [];
  const t = TIER.hand;
  const w = t.w;
  const h = t.h;
  const n = cards.length;
  const natural = n * w + (n - 1) * M.handGap;
  const minStep = Math.ceil(w * (1 - M.fanMaxOverlap));
  const step =
    natural <= area.w || n === 1
      ? w + M.handGap
      : Math.max(minStep, Math.floor((area.w - w) / (n - 1)));
  const total = w + step * (n - 1);
  const left = area.x + Math.max(0, Math.floor((area.w - total) / 2));
  const top = area.y + Math.max(0, area.h - h);
  return cards.map((card, i) => {
    const lifted = selectedId !== undefined && card.entityId === selectedId;
    return {
      ...card,
      rect: { x: left + i * step, y: lifted ? Math.max(area.y, top - M.handLift) : top, w, h },
    };
  });
}
