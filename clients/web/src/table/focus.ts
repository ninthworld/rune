/**
 * Capability-aware spatial focus engine (issue #301).
 *
 * The shell is built around ONE input model: the same abstract verbs — move focus /
 * select, confirm, inspect, back or cancel, pass — reachable from every input
 * capability (`docs/design/ui-design-notes.md` §Input capability model,
 * ui-requirements §Accessibility and input). This module owns the *move focus* verb.
 *
 * It replaces the earlier flat, DOM-order walk of every enabled button with a
 * REGION-aware spatial model: focus moves BETWEEN the shell's regions (the stable
 * `RegionId`s the layout function positions) and WITHIN a region's focusable items,
 * so focus travels the way the table is actually laid out rather than in source
 * order. Tab order stays native and untrapped — this model rides on the arrow keys
 * (and, as an additive follow-up, a d-pad / stick), never on Tab.
 *
 * Design constraints that shape it:
 *
 * - **Input-agnostic.** The engine speaks in {@link FocusDir} (`up`/`down`/`left`/
 *   `right`) and DOM elements only. The keyboard adapter maps arrow keys → a dir; a
 *   future Gamepad API adapter maps the d-pad / left stick → the same dir and calls
 *   the same {@link nextFocus}. No input device is named below this line.
 * - **Robust to sibling redesigns.** Wave-3 issues (#296 HUDs, #297 indicator, #299
 *   rail) restructure the very regions this navigates. So the engine keys off the
 *   shell's `RegionId` (a container tagged `data-focus-region`) plus *generic*
 *   focusable-item discovery inside a region — never each region's internal markup.
 * - **Geometry from the layout function.** Region ordering and adjacency come from
 *   the `layout()` rects (real viewport numbers), not from the DOM box model, so the
 *   model is deterministic even where the DOM reports no geometry (jsdom / SSR).
 *
 * Movement model
 * --------------
 * Each region has an orientation derived from its layout rect: a wide rect is a
 * `row` (its items flow horizontally — the HUD strip, the hand, the action tray),
 * a tall rect is a `column` (the stack/activity rail). For the region under focus:
 *
 * - **Along-axis** (Left/Right in a row, Up/Down in a column) steps to the previous
 *   / next item in the region. Stepping off either end crosses to the next region in
 *   reading order (top-to-bottom, then left-to-right) and lands on its near edge, so
 *   the along-axis walks a region's items and then carries on into the next region.
 * - **Cross-axis** (Up/Down in a row, Left/Right in a column) jumps to the spatially
 *   nearest region in that direction by the layout rects — e.g. Down from the HUD
 *   strip drops onto the battlefield, Left from the rail returns to the board — and,
 *   at an edge with nothing that way, falls back to the next region in reading order.
 *
 * Together these keep every focusable item reachable through region navigation: a
 * region's own axis walks its items, and either axis moves between regions (so a
 * column region like the rail is entered from its neighbours and its items walked
 * with Up/Down). No item is stranded and focus is never trapped.
 *
 * ── Verb → gamepad mapping (groundwork; NOT shipped here) ────────────────────────
 * The follow-up Gamepad API adapter is purely additive — it produces the same verbs
 * this engine and `Table` already consume, so no focus logic changes to add it:
 *
 *   move focus  → D-pad ↑↓←→ and left stick → {@link FocusDir} → {@link nextFocus}
 *   select      → A / cross            → activate the focused control (Enter/Space)
 *   confirm     → A / cross            → the same activation (primary pending action)
 *   inspect     → Y / triangle         → inspect the focused entity (`I`)
 *   back/cancel → B / circle           → back out of the topmost surface (Escape)
 *   pass        → X / square           → pass priority when offered (`P`)
 *   help        → Start / Options      → toggle the shortcut reference (`?`)
 *
 * Keeping the engine input-agnostic is the whole point: the adapter is a thin event
 * source, not a second navigation implementation.
 */
import type { Rect } from './scene';

/** A direction the *move focus* verb can travel. Input-agnostic: arrow keys and a
 * gamepad d-pad / stick both resolve to one of these. */
export type FocusDir = 'up' | 'down' | 'left' | 'right';

/** A discovered region: its stable id, its layout geometry, and the focusable items
 * currently inside it (in DOM order). Only regions with at least one item appear. */
export interface FocusRegion {
  id: string;
  rect: Rect;
  items: HTMLElement[];
}

/**
 * The elements a region exposes to spatial focus. Every interactive control is a
 * real `<button>` (the select-then-confirm affordance everywhere); an escape hatch
 * `[data-focus-item]` lets a non-button control opt in without the engine knowing
 * its markup. Disabled controls are excluded — a binding stays inert when there is
 * no matching action.
 */
const FOCUSABLE_SELECTOR = [
  'button:not([disabled])',
  '[data-focus-item]:not([disabled]):not([aria-disabled="true"])',
].join(', ');

/** A rect's center point. */
function center(rect: Rect): { x: number; y: number } {
  return { x: rect.x + rect.w / 2, y: rect.y + rect.h / 2 };
}

/** Whether a region's items flow horizontally (`row`) or vertically (`column`),
 * read straight off its layout rect — a wide region is a row, a tall one a column. */
function isRow(rect: Rect): boolean {
  return rect.w >= rect.h;
}

/** Read a live DOM rect as a layout {@link Rect} (fallback only; zero in jsdom). */
function domRect(el: HTMLElement): Rect {
  const r = el.getBoundingClientRect();
  return { x: r.x, y: r.y, w: r.width, h: r.height };
}

/**
 * Discover the shell's focusable regions under `root`, ordered top-to-bottom then
 * left-to-right (reading order). Each `[data-focus-region]` container contributes
 * its id, its geometry (from the supplied layout `geometry` map, falling back to the
 * live DOM box only when the layout has no rect for it), and its focusable items in
 * DOM order. Regions with no focusable item are dropped so navigation never lands on
 * an empty region.
 */
export function collectFocusRegions(root: ParentNode, geometry: Map<string, Rect>): FocusRegion[] {
  const containers = Array.from(root.querySelectorAll<HTMLElement>('[data-focus-region]'));
  const regions: FocusRegion[] = [];
  for (const container of containers) {
    const id = container.dataset.focusRegion;
    if (!id) continue;
    const items = Array.from(container.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR));
    if (items.length === 0) continue;
    const rect = geometry.get(id) ?? domRect(container);
    regions.push({ id, rect, items });
  }
  regions.sort((a, b) => a.rect.y - b.rect.y || a.rect.x - b.rect.x);
  return regions;
}

/** The item a region is entered on when crossing into it in `dir`: its first item
 * when arriving from before it (moving right/down), its last when arriving from
 * after it (moving left/up). */
function entryItem(region: FocusRegion, dir: FocusDir): HTMLElement {
  const forward = dir === 'right' || dir === 'down';
  return forward ? region.items[0] : region.items[region.items.length - 1];
}

/** The nearest region strictly in `dir` from `from` (by rect centers), or null when
 * nothing lies that way. Used for the cross-axis jump between regions. */
function nearestRegionInDir(
  regions: FocusRegion[],
  from: FocusRegion,
  dir: FocusDir,
): FocusRegion | null {
  const a = center(from.rect);
  let best: FocusRegion | null = null;
  let bestDist = Infinity;
  for (const region of regions) {
    if (region === from) continue;
    const b = center(region.rect);
    const inDir =
      (dir === 'up' && b.y < a.y) ||
      (dir === 'down' && b.y > a.y) ||
      (dir === 'left' && b.x < a.x) ||
      (dir === 'right' && b.x > a.x);
    if (!inDir) continue;
    // Distance weights the travel axis so a region dead-ahead beats one off to the
    // side at the same reach.
    const dx = b.x - a.x;
    const dy = b.y - a.y;
    const along = dir === 'up' || dir === 'down' ? Math.abs(dy) : Math.abs(dx);
    const across = dir === 'up' || dir === 'down' ? Math.abs(dx) : Math.abs(dy);
    const dist = along + across * 2;
    if (dist < bestDist) {
      bestDist = dist;
      best = region;
    }
  }
  return best;
}

/** The near-edge item of the next non-empty region in reading order from index `ri`,
 * stepping `step` (+1 forward / -1 back) and wrapping — used when an along-axis step
 * overflows the current region's items. Returns null only when no region has items. */
function crossInReadingOrder(regions: FocusRegion[], ri: number, step: 1 | -1): HTMLElement | null {
  const n = regions.length;
  if (n === 0) return null;
  for (let hop = 1; hop <= n; hop += 1) {
    const idx = (((ri + step * hop) % n) + n) % n;
    const region = regions[idx];
    if (region.items.length > 0) {
      return step > 0 ? region.items[0] : region.items[region.items.length - 1];
    }
  }
  return null;
}

/**
 * Resolve the *move focus* verb: the element that should receive focus when moving
 * `dir` from `active`, or null when nothing can. Pure over the supplied regions —
 * the caller decides whether to `preventDefault` and `focus()` the result.
 *
 * With nothing focused (or focus outside any region), a forward direction enters the
 * first region and a backward direction the last, so the first arrow press always
 * lands somewhere sensible.
 */
export function nextFocus(
  regions: FocusRegion[],
  active: Element | null,
  dir: FocusDir,
): HTMLElement | null {
  if (regions.length === 0) return null;

  const ri = regions.findIndex(
    (region) => active instanceof HTMLElement && region.items.includes(active),
  );
  if (ri === -1) {
    const forward = dir === 'right' || dir === 'down';
    const region = forward ? regions[0] : regions[regions.length - 1];
    return entryItem(region, dir);
  }

  const region = regions[ri];
  const ii = region.items.indexOf(active as HTMLElement);
  const row = isRow(region.rect);
  const alongForward = row ? dir === 'right' : dir === 'down';
  const alongBackward = row ? dir === 'left' : dir === 'up';

  if (alongForward || alongBackward) {
    const step: 1 | -1 = alongForward ? 1 : -1;
    const ni = ii + step;
    if (ni >= 0 && ni < region.items.length) return region.items[ni];
    // Off the end of this region: continue into the next region in reading order so
    // one axis linearises the whole table (every item stays reachable).
    return crossInReadingOrder(regions, ri, step);
  }

  // Cross-axis: jump to the spatially nearest region that way. With nothing in that
  // direction (an edge region), fall back to the next region in reading order so a
  // press at the board's edge still progresses rather than dead-ending — this is what
  // keeps every region reachable regardless of the axis a player favours.
  const target = nearestRegionInDir(regions, region, dir);
  if (target) return entryItem(target, dir);
  const step: 1 | -1 = dir === 'right' || dir === 'down' ? 1 : -1;
  return crossInReadingOrder(regions, ri, step);
}
