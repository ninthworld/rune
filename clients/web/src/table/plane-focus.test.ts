import { describe, expect, it } from 'vitest';
import { seatTable, menagerie, stage } from './plane.fixture';

describe('stagePlane focus model (issue #478, layout-model §Focus model)', () => {
  it('stages exactly one focused opponent at 3+ players', () => {
    for (const opponents of [2, 3, 4, 5]) {
      const plane = stage(seatTable({ opponents, active: 'p2' }));
      expect(plane.focusSeat).toBe('p2');
      expect(plane.farSide?.focused).toBe(true);
      expect(plane.wings.every((w) => !w.focused)).toBe(true);
      expect(plane.wings).toHaveLength(opponents - 1);
    }
  });

  it('defaults focus to the active opponent during their turn', () => {
    const plane = stage(seatTable({ opponents: 3, active: 'p3' }));
    expect(plane.focusSeat).toBe('p3');
    expect(plane.wings.map((w) => w.seat)).toEqual(['p2', 'p4']);
  });

  it("defaults focus to the next opponent in turn order on the receiver's turn", () => {
    const plane = stage(seatTable({ opponents: 3, active: 'p1' }));
    expect(plane.focusSeat).toBe('p2');
    // The same table with a scrambled seat order follows that order instead.
    const scrambled = stage(
      seatTable({ opponents: 3, active: 'p1', seatOrder: ['p1', 'p4', 'p3', 'p2'] }),
    );
    expect(scrambled.focusSeat).toBe('p4');
  });

  it('skips an eliminated seat when deriving default focus', () => {
    const plane = stage(seatTable({ opponents: 3, active: 'p1', eliminated: ['p2'] }));
    expect(plane.focusSeat).toBe('p3');
    // The eliminated seat still holds its wing slot.
    expect(plane.wings.map((w) => w.seat)).toEqual(['p2', 'p4']);
  });

  it('re-stages manual focus into the far side (ephemeral, per call)', () => {
    const view = seatTable({ opponents: 3, active: 'p2' });
    const manual = stage(view, undefined, { focusSeat: 'p4' });
    expect(manual.focusSeat).toBe('p4');
    expect(manual.farSide?.seat).toBe('p4');
    expect(manual.wings.map((w) => w.seat)).toEqual(['p2', 'p3']);
    // Dropped state re-derives the default — manual focus is never sticky.
    expect(stage(view).focusSeat).toBe('p2');
  });

  it('allows manual focus on an eliminated seat (public zones stay browsable)', () => {
    const view = seatTable({ opponents: 3, active: 'p2', eliminated: ['p4'] });
    const plane = stage(view, undefined, { focusSeat: 'p4' });
    expect(plane.farSide?.seat).toBe('p4');
    expect(plane.farSide?.eliminated).toBe(true);
  });

  it('ignores a manual focus that names no opponent', () => {
    const view = seatTable({ opponents: 3, active: 'p2' });
    expect(stage(view, undefined, { focusSeat: 'p1' }).focusSeat).toBe('p2');
    expect(stage(view, undefined, { focusSeat: 'p9' }).focusSeat).toBe('p2');
  });

  it('stages the first candidate-bearing board as context for a prompt', () => {
    const view = seatTable({
      opponents: 3,
      active: 'p1',
      perms: [...menagerie('p3', 2), ...menagerie('p4', 2)],
    });
    // Candidates sit on p4 and p3; the first bearing seat in seat order stages.
    const plane = stage(view, undefined, { candidates: ['p4_beast_0', 'p3_beast_1'] });
    expect(plane.focusSeat).toBe('p3');
    // Manual focus still wins over the candidate-bearing default.
    const manual = stage(view, undefined, {
      focusSeat: 'p2',
      candidates: ['p4_beast_0', 'p3_beast_1'],
    });
    expect(manual.focusSeat).toBe('p2');
    // The unfocused candidate still pierces its wing — no focus change needed.
    const wing = manual.wings.find((w) => w.seat === 'p4')!;
    expect(wing.renders.some((r) => r.entityId === 'p4_beast_0' && r.candidate)).toBe(true);
  });

  it('has no focus concept in a duel', () => {
    const plane = stage(seatTable({ opponents: 1, active: 'p2' }));
    expect(plane.focusSeat).toBeUndefined();
    expect(plane.farSide?.seat).toBe('p2');
    expect(plane.farSide?.focused).toBe(false);
  });

  it('marks attacked, active, and priority seats on their staged regions', () => {
    const view = seatTable({
      opponents: 3,
      active: 'p2',
      perms: [{ id: 'p2_atk', controller: 'p2', attacking: true, attacking_player: 'p3' }],
    });
    const plane = stage(view);
    expect(plane.farSide?.active).toBe(true);
    // Combat against a wing seat is never silent: the wing wears the ring.
    expect(plane.wings.find((w) => w.seat === 'p3')?.attacked).toBe(true);
    expect(plane.wings.find((w) => w.seat === 'p4')?.attacked).toBe(false);
  });
});
