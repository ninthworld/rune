import { describe, expect, it } from 'vitest';
import type { CardView } from '../protocol';
import {
  ZONE_BROWSER,
  clampBlock,
  zoneBrowserEntries,
  zoneBrowserPlan,
  zoneBrowserRange,
} from './zoneBrowserView';

/** A pile of `n` distinguishable cards in wire order (index 0 is the bottom). */
function pile(n: number): CardView[] {
  return Array.from({ length: n }, (_, i) => ({
    id: `c${i}`,
    name: `Card ${i}`,
    type_line: 'Instant',
  }));
}

describe('zone browser presentation model (issue #584)', () => {
  it('presents the pile top-first while keeping the server order intact', () => {
    const cards = pile(3);
    const { entries } = zoneBrowserEntries(cards);

    // Wire order is top-of-pile-last, so the top card is presented first…
    expect(entries.map((e) => e.card.id)).toEqual(['c2', 'c1', 'c0']);
    // …and each entry still knows exactly where it sits on the wire.
    expect(entries.map((e) => e.pileIndex)).toEqual([2, 1, 0]);
    expect(entries.map((e) => e.fromTop)).toEqual([1, 2, 3]);
    // The server's array is never touched.
    expect(cards.map((c) => c.id)).toEqual(['c0', 'c1', 'c2']);
  });

  it('reproduces the identical presentation from the same pile (reconnect)', () => {
    const cards = pile(7);
    expect(zoneBrowserEntries(cards)).toEqual(zoneBrowserEntries(cards.slice()));
  });

  it('presents an empty pile as one empty block', () => {
    const { plan, entries } = zoneBrowserEntries([]);
    expect(entries).toEqual([]);
    expect(plan).toEqual({ total: 0, blocks: 1, paged: false });
  });

  it('keeps a pile at the cap in a single unpaged block', () => {
    const plan = zoneBrowserPlan(ZONE_BROWSER.block);
    expect(plan.blocks).toBe(1);
    expect(plan.paged).toBe(false);
    expect(zoneBrowserRange(plan, 0)).toEqual({ start: 0, end: ZONE_BROWSER.block });
  });

  it('bounds mounted faces by the cap however large the pile is', () => {
    const cards = pile(400);
    const plan = zoneBrowserPlan(cards.length);
    expect(plan.paged).toBe(true);
    expect(plan.blocks).toBe(Math.ceil(400 / ZONE_BROWSER.block));
    for (let block = 0; block < plan.blocks; block += 1) {
      expect(zoneBrowserEntries(cards, block).entries.length).toBeLessThanOrEqual(
        ZONE_BROWSER.block,
      );
    }
  });

  it('covers every card exactly once across the blocks, in top-first order', () => {
    const cards = pile(250);
    const plan = zoneBrowserPlan(cards.length);
    const seen = Array.from({ length: plan.blocks }, (_, block) =>
      zoneBrowserEntries(cards, block).entries.map((e) => e.card.id),
    ).flat();
    expect(seen).toEqual(
      cards
        .map((c) => c.id)
        .slice()
        .reverse(),
    );
  });

  it('clamps a block that does not exist rather than rendering nothing', () => {
    const cards = pile(5);
    expect(clampBlock(zoneBrowserPlan(5), 9)).toBe(0);
    expect(clampBlock(zoneBrowserPlan(5), -3)).toBe(0);
    expect(clampBlock(zoneBrowserPlan(5), Number.NaN)).toBe(0);
    // A pile that shrank under a shown block falls back to a block that exists.
    expect(zoneBrowserEntries(cards, 4).block).toBe(0);
    expect(zoneBrowserEntries(cards, 4).entries).toHaveLength(5);
  });
});
