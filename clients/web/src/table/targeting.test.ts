import { describe, expect, it } from 'vitest';
import type { ValidAction } from '../protocol';
import {
  activeCandidates,
  activeRequirement,
  assembleTargets,
  beginTargeting,
  isComplete,
  pick,
  requiresTargets,
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

describe('purity', () => {
  it('never mutates the session it is given', () => {
    const s0 = beginTargeting(bolt);
    const snapshot = structuredClone(s0);
    pick(s0, 'perm_a');
    expect(s0).toEqual(snapshot);
  });
});
