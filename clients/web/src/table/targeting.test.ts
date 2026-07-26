import { describe, expect, it } from 'vitest';
import type { ValidAction } from '../protocol';
import {
  activeCandidates,
  activeRequirement,
  assembleTargets,
  beginTargeting,
  canRetract,
  isComplete,
  pick,
  requiresTargets,
  retract,
} from './targeting';

/** A single-target spell action (Lightning Bolt), as the server would issue it. */
const bolt: ValidAction = {
  id: 'a3',
  type: 'cast_spell',
  label: 'Cast Lightning Bolt',
  subject: ['c3'],
  token: 'h:9f2c',
  requirements: [{ slot: 't0', prompt: 'target creature or player', candidates: ['perm_a', 'p2'] }],
};

/** A two-target action to exercise the multi-slot prompt queue. */
const twinBolt: ValidAction = {
  id: 'a4',
  type: 'cast_spell',
  label: 'Twin Bolt',
  subject: ['c4'],
  token: 'h:aaaa',
  requirements: [
    { slot: 't0', prompt: 'target creature', candidates: ['perm_a', 'perm_b'] },
    { slot: 't1', prompt: 'target creature', candidates: ['perm_a', 'perm_b'] },
  ],
};

const plain: ValidAction = { id: 'a1', type: 'pass_priority', label: 'Pass' };

describe('requiresTargets', () => {
  it('is true only when the action carries requirement slots', () => {
    expect(requiresTargets(bolt)).toBe(true);
    expect(requiresTargets(plain)).toBe(false);
    expect(requiresTargets({ ...bolt, requirements: [] })).toBe(false);
  });
});

describe('single-slot targeting session', () => {
  it('walks the one slot and assembles the atomic answer', () => {
    const s0 = beginTargeting(bolt);
    expect(isComplete(s0)).toBe(false);
    expect(activeRequirement(s0)?.slot).toBe('t0');
    expect(activeCandidates(s0)).toEqual(['perm_a', 'p2']);
    // No answer while incomplete.
    expect(assembleTargets(s0)).toBeNull();

    const s1 = pick(s0, 'perm_a');
    expect(isComplete(s1)).toBe(true);
    expect(activeRequirement(s1)).toBeNull();
    expect(activeCandidates(s1)).toEqual([]);
    expect(assembleTargets(s1)).toEqual([{ slot: 't0', chosen: ['perm_a'] }]);
  });

  it('ignores extra picks once complete (no over-filling)', () => {
    const done = pick(beginTargeting(bolt), 'p2');
    expect(pick(done, 'perm_a')).toBe(done);
  });
});

describe('multi-slot targeting session', () => {
  it('advances slot by slot and keys each answer to its slot', () => {
    let s = beginTargeting(twinBolt);
    expect(activeRequirement(s)?.slot).toBe('t0');
    s = pick(s, 'perm_a');
    // Now on the second slot; still incomplete.
    expect(activeRequirement(s)?.slot).toBe('t1');
    expect(isComplete(s)).toBe(false);
    expect(assembleTargets(s)).toBeNull();

    s = pick(s, 'perm_b');
    expect(isComplete(s)).toBe(true);
    expect(assembleTargets(s)).toEqual([
      { slot: 't0', chosen: ['perm_a'] },
      { slot: 't1', chosen: ['perm_b'] },
    ]);
  });
});

describe('retract (§8 one-step UNDO)', () => {
  it('takes back one pick of a multi-target action, keeping the action and the rest', () => {
    // The whole point of the control: a mis-click on target 2 of 3 costs one
    // re-pick, not the session. Three slots, so there is a pick both before and
    // after the retracted one.
    const tripleBolt: ValidAction = {
      ...twinBolt,
      requirements: [
        { slot: 't0', prompt: 'target creature', candidates: ['perm_a', 'perm_b', 'perm_c'] },
        { slot: 't1', prompt: 'target creature', candidates: ['perm_a', 'perm_b', 'perm_c'] },
        { slot: 't2', prompt: 'target creature', candidates: ['perm_a', 'perm_b', 'perm_c'] },
      ],
    };
    let s = pick(pick(beginTargeting(tripleBolt), 'perm_a'), 'perm_b');
    expect(canRetract(s)).toBe(true);

    s = retract(s);
    // The first pick survives, the second is gone, and the session is reopened
    // on the slot that was retracted — not abandoned.
    expect(s.action).toBe(tripleBolt);
    expect(s.picks).toEqual([['perm_a']]);
    expect(activeRequirement(s)?.slot).toBe('t1');
    expect(isComplete(s)).toBe(false);

    // Re-picking that slot and finishing assembles the full answer.
    s = pick(pick(s, 'perm_c'), 'perm_a');
    expect(assembleTargets(s)).toEqual([
      { slot: 't0', chosen: ['perm_a'] },
      { slot: 't1', chosen: ['perm_c'] },
      { slot: 't2', chosen: ['perm_a'] },
    ]);
  });

  it('reopens the first slot rather than ending the session', () => {
    const s = retract(pick(beginTargeting(twinBolt), 'perm_a'));
    expect(canRetract(s)).toBe(false);
    expect(s.picks).toEqual([]);
    // Still targeting the same action: leaving it is CANCEL's job, not UNDO's.
    expect(s.action).toBe(twinBolt);
    expect(activeRequirement(s)?.slot).toBe('t0');
  });

  it('is a no-op with nothing picked', () => {
    const s0 = beginTargeting(bolt);
    expect(canRetract(s0)).toBe(false);
    expect(retract(s0)).toBe(s0);
  });
});

describe('purity', () => {
  it('never mutates the session it is given', () => {
    const s0 = beginTargeting(bolt);
    const snapshot = structuredClone(s0);
    pick(s0, 'perm_a');
    expect(s0).toEqual(snapshot);

    const s1 = pick(s0, 'perm_a');
    const picked = structuredClone(s1);
    retract(s1);
    expect(s1).toEqual(picked);
  });
});
