import { describe, expect, it } from 'vitest';
import { mergeDeckRows, type DeckRowModel, type DeckRowRender } from './deckRowTransitions';

/** A model row with defaults, for terse fixtures. */
function model(id: string, count: number, isCommanderCandidate = false): DeckRowModel {
  return { id, name: id, count, isCommanderCandidate };
}

/** A present render row, as it would sit in the current list. */
function present(id: string, count: number, changeSeq = 0): DeckRowRender {
  return { ...model(id, count), phase: 'present', changeSeq };
}

describe('mergeDeckRows (enter / copy-change / leave distinctions)', () => {
  it('enters a brand-new card as a present row at change-seq 0', () => {
    const { rows, leaving } = mergeDeckRows([], [model('shock', 1)], true);
    expect(rows).toEqual([present('shock', 1, 0)]);
    expect(leaving).toEqual([]);
  });

  it('ticks change-seq when a persisting row’s copy count changes', () => {
    const first = mergeDeckRows([present('shock', 1)], [model('shock', 2)], true);
    expect(first.rows[0]).toMatchObject({ id: 'shock', count: 2, phase: 'present', changeSeq: 1 });
    // Another change ticks again — distinguishing repeated copy edits from the mount.
    const second = mergeDeckRows(first.rows, [model('shock', 1)], true);
    expect(second.rows[0]).toMatchObject({ count: 1, changeSeq: 2 });
  });

  it('leaves change-seq untouched when nothing about the row changed', () => {
    const { rows } = mergeDeckRows([present('shock', 2, 3)], [model('shock', 2)], true);
    expect(rows[0].changeSeq).toBe(3);
  });

  it('holds a removed row as leaving (exit can play) and reports it', () => {
    const { rows, leaving } = mergeDeckRows([present('shock', 1)], [], true);
    expect(rows).toEqual([{ ...present('shock', 1), phase: 'leaving' }]);
    expect(leaving).toEqual(['shock']);
  });

  it('drops a removed row outright when leaving is disabled (reduced motion)', () => {
    const { rows, leaving } = mergeDeckRows([present('shock', 1)], [], false);
    expect(rows).toEqual([]);
    expect(leaving).toEqual([]);
  });

  it('keeps an already-leaving row until its own timer removes it', () => {
    const leavingRow: DeckRowRender = { ...present('shock', 1), phase: 'leaving' };
    const { rows, leaving } = mergeDeckRows([leavingRow], [], true);
    expect(rows).toEqual([leavingRow]);
    // It is not re-reported — its removal timer is already scheduled.
    expect(leaving).toEqual([]);
  });

  it('revives a leaving row that comes back before its exit finishes, ticking the seq', () => {
    const leavingRow: DeckRowRender = { ...present('shock', 1, 4), phase: 'leaving' };
    const { rows, leaving } = mergeDeckRows([leavingRow], [model('shock', 1)], true);
    expect(rows[0]).toMatchObject({ phase: 'present', changeSeq: 5 });
    expect(leaving).toEqual([]);
  });

  it('preserves order: persisting rows keep their slot, new cards append', () => {
    const current = [present('a', 1), present('b', 1)];
    const { rows } = mergeDeckRows(current, [model('a', 1), model('b', 1), model('c', 1)], true);
    expect(rows.map((r) => r.id)).toEqual(['a', 'b', 'c']);
  });
});
