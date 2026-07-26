/**
 * Pure presentation model for the public zone browser (issue #584).
 *
 * The browser is a render of one {@link ZonePile} the server already sent, so
 * everything it has to decide — which way a human reads the pile, and how many
 * card faces may exist at once — is arithmetic over `cards.length`, testable
 * without a DOM. The component does nothing this module does not describe.
 *
 * ## Reading direction
 *
 * The wire order is **top of the pile last** (`ZonePile.cards`), which is the
 * order a physical pile stacks in and the order the server must keep. A grid a
 * human scans reads the other way: the card that just went to the graveyard is
 * the one they are looking for, so {@link zoneBrowserEntries} presents the pile
 * **top-first** and carries each card's untouched wire index along with it.
 * Nothing is re-derived and nothing is dropped — the presented sequence is a
 * reversal of the server's list and states itself in the heading
 * ({@link ZONE_BROWSER_ORDER_NOTE}), which is what `docs/design/zone-geography.md`
 * §8.2 now requires of whichever direction is chosen.
 *
 * ## Bounded DOM
 *
 * `zone-geography.md` §8.2.4 caps the browser at 120 entries before it has to
 * stop spending the scene's DOM budget. A card face is ~14 nodes at the browse
 * tier, so a 400-card exile drawn whole is several thousand nodes for a surface
 * the player scrolls past. The browser therefore renders **one block of at most
 * {@link ZONE_BROWSER.block} entries at a time**: a pile at or under the cap is
 * one block (it simply scrolls, as it does today), and a larger pile pages
 * through blocks with ordinary ≥ 44 px controls. Mounted faces are bounded by
 * the cap in every case, and the block is ephemeral presentation state — a fresh
 * mount from the same view reproduces block 0 exactly.
 */
import type { CardView } from '../protocol';

/** The browser's fixed numbers. */
export const ZONE_BROWSER = {
  /**
   * The largest number of card faces mounted at once (`zone-geography.md`
   * §8.2.4's 120-entry virtualization threshold, read as a hard ceiling).
   */
  block: 120,
} as const;

/** The heading's statement of reading direction — the browser always says it. */
export const ZONE_BROWSER_ORDER_NOTE = 'top of pile first';

/** How a pile divides into mountable blocks. */
export interface ZoneBrowserPlan {
  /** Total cards in the pile, exactly as the server sent them. */
  total: number;
  /** How many blocks the pile occupies (always ≥ 1, so an empty pile has one). */
  blocks: number;
  /** Whether the pile is larger than one block and so shows block controls. */
  paged: boolean;
}

/** One presented entry: the server's card plus where it sits in the pile. */
export interface ZoneBrowserEntry {
  /** The `CardView` the server sent, untouched. */
  card: CardView;
  /** Its index in the wire order (`0` is the bottom of the pile). */
  pileIndex: number;
  /** Its distance from the top of the pile (`1` is the top card). */
  fromTop: number;
}

/** Divide a pile into blocks. Pure arithmetic over the server's count. */
export function zoneBrowserPlan(total: number): ZoneBrowserPlan {
  const blocks = Math.max(1, Math.ceil(total / ZONE_BROWSER.block));
  return { total, blocks, paged: blocks > 1 };
}

/** Clamp a block index into the plan; out-of-range input can only ever clamp. */
export function clampBlock(plan: ZoneBrowserPlan, block: number): number {
  if (!Number.isFinite(block)) return 0;
  return Math.max(0, Math.min(Math.trunc(block), plan.blocks - 1));
}

/**
 * The half-open range of **presented** positions one block covers. Position 0 is
 * the top of the pile, matching {@link zoneBrowserEntries}.
 */
export function zoneBrowserRange(
  plan: ZoneBrowserPlan,
  block: number,
): { start: number; end: number } {
  const clamped = clampBlock(plan, block);
  const start = clamped * ZONE_BROWSER.block;
  return { start, end: Math.min(plan.total, start + ZONE_BROWSER.block) };
}

/**
 * The entries one block presents, **top of the pile first**.
 *
 * The server's list is never mutated and never filtered: entry *i* of the whole
 * presentation is `cards[cards.length - 1 - i]`, and each entry carries that
 * source index back, so a caller can always recover the wire position it came
 * from. Replaying the same view therefore reproduces the identical sequence.
 */
export function zoneBrowserEntries(
  cards: readonly CardView[],
  block = 0,
): { plan: ZoneBrowserPlan; block: number; entries: ZoneBrowserEntry[] } {
  const plan = zoneBrowserPlan(cards.length);
  const clamped = clampBlock(plan, block);
  const { start, end } = zoneBrowserRange(plan, clamped);
  const entries: ZoneBrowserEntry[] = [];
  for (let position = start; position < end; position += 1) {
    const pileIndex = cards.length - 1 - position;
    entries.push({ card: cards[pileIndex]!, pileIndex, fromTop: position + 1 });
  }
  return { plan, block: clamped, entries };
}
