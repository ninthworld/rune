import { describe, expect, it } from 'vitest';
import type { GameView } from '../protocol';
import { PHONE, seatTable, bears, menagerie, stage } from './plane.fixture';

/** A 4-player table (focused p2) with the given battlefield. */
function fourSeat(
  perms: Parameters<typeof seatTable>[0]['perms'],
  extra?: Partial<Parameters<typeof seatTable>[0]>,
) {
  return seatTable({ opponents: 3, active: 'p2', perms, ...extra });
}

describe('stagePlane degradation ladder (issue #478, layout-model §Ladder)', () => {
  it('stays at rung 0 for a sparse board', () => {
    const plane = stage(fourSeat(menagerie('p2', 4)));
    expect(plane.farSide?.rung).toBe(0);
    expect(plane.farSide?.surface).toBe('support');
    expect(plane.farSide?.renders).toHaveLength(4);
  });

  it('steps the tier down one rung when the full tier overflows (rung 1)', () => {
    const plane = stage(fourSeat(menagerie('p2', 11)));
    expect(plane.farSide?.rung).toBe(1);
    // The step-down is a real tier change: mini, one rung under support.
    expect(plane.farSide?.surface).toBe('mini');
    expect(plane.farSide?.renders.every((r) => r.tier === 'mini')).toBe(true);
  });

  it('folds identical-full-state permanents into one ×N pile (rung 2)', () => {
    const plane = stage(fourSeat(bears('p2', 30)));
    expect(plane.farSide?.rung).toBe(2);
    expect(plane.farSide?.renders).toHaveLength(1);
    const pile = plane.farSide!.renders[0]!;
    expect(pile.stackCount).toBe(30);
    expect(pile.memberIds).toHaveLength(30);
    // The representative is the first member in server order (stable identity).
    expect(pile.entityId).toBe('p2_bear_0');
  });

  it('never folds across differing visual state (tapped splits the pile)', () => {
    const perms = [...bears('p2', 10), ...bears('p2', 3, { tapped: true, prefix: 'tapped' })];
    // Also crowd the slot so the ladder actually reaches the folding rung.
    perms.push(...menagerie('p2', 10));
    const plane = stage(fourSeat(perms));
    expect(plane.farSide?.rung).toBe(2);
    const piles = plane.farSide!.renders.filter((r) => r.stackCount > 1);
    expect(piles.map((p) => p.stackCount).sort((a, b) => a - b)).toEqual([3, 10]);
    expect(piles.find((p) => p.stackCount === 3)?.tapped).toBe(true);
  });

  it('carries the offered-action fingerprint in the ×N grouping key', () => {
    const perms = [...bears('p2', 20)];
    const actions: GameView['valid_actions'] = [
      {
        id: 'a1',
        type: 'activate_ability',
        label: 'Tap: Add {G}',
        subject: ['p2_bear_0', 'p2_bear_1'],
      },
    ];
    const plane = stage(fourSeat(perms, { validActions: actions }));
    // Same visual state, different offered actions: two piles, never one.
    const piles = plane.farSide!.renders;
    expect(piles.map((p) => p.stackCount).sort((a, b) => a - b)).toEqual([2, 18]);
    // Every member stays accounted for.
    expect(piles.flatMap((p) => p.memberIds)).toHaveLength(20);
  });

  it('wraps rows inside the fixed slot when folding is not enough (rung 3)', () => {
    const plane = stage(fourSeat(menagerie('p2', 16)));
    expect(plane.farSide?.rung).toBe(3);
    expect(plane.farSide?.renders).toHaveLength(16);
    // Wrapping produced more than one line…
    const ys = new Set(plane.farSide!.renders.map((r) => r.rect.y));
    expect(ys.size).toBeGreaterThan(1);
    // …and traded row height inside the slot, not neighbor space.
    const slot = plane.farSide!.rect;
    for (const render of plane.farSide!.renders) {
      expect(render.rect.x).toBeGreaterThanOrEqual(slot.x);
      expect(render.rect.x + render.rect.w).toBeLessThanOrEqual(slot.x + slot.w);
    }
  });

  it('engages the ladder per region, independently', () => {
    const plane = stage(
      fourSeat([...bears('p3', 40), ...menagerie('p2', 3), ...menagerie('p1', 3)]),
    );
    // p3's crowded wing degrades; the sparse far side and receiver do not.
    const wing = plane.wings.find((w) => w.seat === 'p3');
    expect(wing?.rung).toBeGreaterThan(0);
    expect(plane.farSide?.rung).toBe(0);
    expect(plane.receiver?.rung).toBe(0);
  });

  it('never digests the receiver or the far side', () => {
    const plane = stage(fourSeat([...menagerie('p1', 40), ...menagerie('p2', 40)]));
    for (const region of [plane.receiver!, plane.farSide!]) {
      expect(region.rung).toBeLessThanOrEqual(3);
      expect(region.digest).toBeUndefined();
      // Every permanent keeps an individually addressable render.
      expect(region.renders).toHaveLength(40);
    }
  });
});

describe('stagePlane wing digest (rung 4, all-category counts)', () => {
  it('digests a wing that cannot fit its board, with all categories counted', () => {
    const perms = [
      ...menagerie('p3', 20),
      { id: 'p3_art', controller: 'p3', type_line: 'Artifact', name: 'Urn' },
      { id: 'p3_ench', controller: 'p3', type_line: 'Enchantment', name: 'Glow' },
      { id: 'p3_pw', controller: 'p3', type_line: 'Legendary Planeswalker — Xa', name: 'Xa' },
      ...Array.from({ length: 3 }, (_, i) => ({
        id: `p3_land_${i}`,
        controller: 'p3',
        type_line: 'Basic Land — Forest',
        name: 'Forest',
      })),
    ];
    const plane = stage(fourSeat(perms));
    const wing = plane.wings.find((w) => w.seat === 'p3')!;
    expect(wing.rung).toBe(4);
    // Every category present is counted; planeswalkers are "other permanents"
    // (layout-model rung 4) — the board can never read as empty.
    expect(wing.digest).toEqual({ creatures: 20, others: 3, lands: 3 });
    // Pile counts stay visible beside the digest.
    expect(wing.zones.library).toBe(60);
  });

  it('digests every wing from the start at 5–6 players', () => {
    const plane = stage(seatTable({ opponents: 5, active: 'p2', perms: menagerie('p4', 2) }));
    const wing = plane.wings.find((w) => w.seat === 'p4')!;
    expect(wing.rung).toBe(4);
    expect(wing.digest).toEqual({ creatures: 2, others: 0, lands: 0 });
    // No card renders at the digest rung without a prompt.
    expect(wing.renders).toHaveLength(0);
  });
});

describe('stagePlane candidate piercing (issue #478, layout-model §Focus)', () => {
  it('renders a candidate individually through the digest rung', () => {
    // The six-player mock's proof: a digest wing renders its ringed candidate.
    // Manual focus stays on p2 — answering the prompt needs no focus change.
    const view = seatTable({ opponents: 5, active: 'p2', perms: menagerie('p5', 6) });
    const plane = stage(view, undefined, { focusSeat: 'p2', candidates: ['p5_beast_2'] });
    const wing = plane.wings.find((w) => w.seat === 'p5')!;
    expect(wing.rung).toBe(4);
    expect(wing.digest?.creatures).toBe(6);
    expect(wing.renders).toHaveLength(1);
    const candidate = wing.renders[0]!;
    expect(candidate.entityId).toBe('p5_beast_2');
    expect(candidate.candidate).toBe(true);
    expect(candidate.stackCount).toBe(1);
    expect(candidate.hitRect.w).toBeGreaterThanOrEqual(44);
    // The candidate stages inside its own wing — answering needs no focus change.
    expect(candidate.rect.x).toBeGreaterThanOrEqual(wing.rect.x);
    expect(candidate.rect.x + candidate.rect.w).toBeLessThanOrEqual(wing.rect.x + wing.rect.w);
  });

  it('never folds a candidate into an ×N pile', () => {
    const view = fourSeat(bears('p2', 30));
    const plane = stage(view, undefined, { candidates: ['p2_bear_7'] });
    const renders = plane.farSide!.renders;
    const candidate = renders.find((r) => r.entityId === 'p2_bear_7')!;
    expect(candidate.candidate).toBe(true);
    expect(candidate.stackCount).toBe(1);
    // The rest still fold; nothing is lost.
    expect(renders.flatMap((r) => r.memberIds)).toHaveLength(30);
  });

  it('never folds combat participants, attachments, or the selection', () => {
    const perms = [
      ...bears('p2', 12),
      ...bears('p2', 2, { prefix: 'atk' }).map((p) => ({ ...p, attacking: true })),
      {
        id: 'p2_aura',
        controller: 'p2',
        type_line: 'Enchantment — Aura',
        attached_to: 'p2_bear_0',
      },
      ...menagerie('p2', 8),
    ];
    const plane = stage(fourSeat(perms), undefined, { selectedId: 'p2_bear_1' });
    const renders = plane.farSide!.renders;
    for (const id of ['atk_bear_0', 'atk_bear_1', 'p2_aura', 'p2_bear_0', 'p2_bear_1']) {
      const render = renders.find((r) => r.memberIds.includes(id))!;
      expect(render.stackCount).toBe(1);
      expect(render.entityId).toBe(id);
    }
  });

  it('grows a candidate strip on a compact summary tile', () => {
    // Manual focus pins p2 so the candidate-bearing seat stays a tile.
    const view = seatTable({ opponents: 3, active: 'p2', perms: menagerie('p4', 3) });
    const plane = stage(view, PHONE, { focusSeat: 'p2', candidates: ['p4_beast_1'] });
    const tile = plane.tiles.find((t) => t.seat === 'p4')!;
    expect(tile.candidates).toHaveLength(1);
    const candidate = tile.candidates[0]!;
    expect(candidate.entityId).toBe('p4_beast_1');
    expect(candidate.candidate).toBe(true);
    expect(candidate.hitRect.w).toBeGreaterThanOrEqual(44);
    // The strip lives inside the grown tile, so the tile stays one touch home.
    expect(candidate.rect.y + candidate.rect.h).toBeLessThanOrEqual(tile.rect.y + tile.rect.h);
    // A tile without candidates keeps its compact height.
    const other = plane.tiles.find((t) => t.seat === 'p3')!;
    expect(other.candidates).toHaveLength(0);
    expect(other.rect.h).toBeLessThan(tile.rect.h);
  });
});
