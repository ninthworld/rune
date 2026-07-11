import { describe, expect, it } from 'vitest';
import type { ValidAction } from '../protocol';
import {
  activeCandidates,
  activeChosen,
  activeSlot,
  advance,
  allSlotsSatisfied,
  assembleChoices,
  beginMultiSelect,
  classifyAction,
  hasOptions,
  isLastSlot,
  isMultiSelect,
  optionsSubmittable,
  toggle,
} from './multiSelect';

/** Declare-attackers: one subset requirement slot. */
const attackers: ValidAction = {
  id: 'a5',
  type: 'declare_attackers',
  label: 'Declare attackers',
  token: 'h:atk0',
  requirements: [{ slot: 'attackers', prompt: 'Choose attackers', candidates: ['atk_1', 'atk_2'] }],
};

/** Declare-blockers: one subset slot per attacker. */
const blockers: ValidAction = {
  id: 'a6',
  type: 'declare_blockers',
  label: 'Declare blockers',
  token: 'h:blk0',
  requirements: [
    {
      slot: 'block_atk_1',
      prompt: 'Choose blockers for Verdant Scout',
      candidates: ['blk_1', 'blk_2'],
    },
    { slot: 'block_atk_2', prompt: 'Choose blockers for Hill Giant', candidates: ['blk_1'] },
  ],
};

/** Mulligan: an option decision plus a count-1 bottoming select_from_zone. */
const mulligan: ValidAction = {
  id: 'a0',
  type: 'mulligan_decision',
  label: 'Keep or mulligan',
  token: 'h:mull',
  prompts: [
    {
      kind: 'option',
      slot: 'decision',
      prompt: 'Keep this hand or take a mulligan?',
      options: [
        { id: 'keep', label: 'Keep this hand' },
        { id: 'mulligan', label: 'Mulligan' },
      ],
    },
    {
      kind: 'select_from_zone',
      slot: 'bottom',
      prompt: 'Put 1 card(s) on the bottom of your library',
      zone: 'hand',
      owner: 'p1',
      count: 1,
      candidates: ['card_a', 'card_b'],
    },
  ],
};

/** A single-target spell — the flow that stays with targeting.ts, not multi-select. */
const bolt: ValidAction = {
  id: 'a3',
  type: 'cast_spell',
  label: 'Cast Lightning Bolt',
  subject: ['c3'],
  token: 'h:9f2c',
  requirements: [{ slot: 't0', prompt: 'target', candidates: ['perm_a', 'p2'] }],
};

const plain: ValidAction = { id: 'a1', type: 'pass_priority', label: 'Pass' };

describe('classifyAction / isMultiSelect', () => {
  it('routes combat declarations and prompt-bearing actions to multi-select', () => {
    expect(classifyAction(attackers)).toBe('multi');
    expect(classifyAction(blockers)).toBe('multi');
    expect(classifyAction(mulligan)).toBe('multi');
    expect(isMultiSelect(attackers)).toBe(true);
  });

  it('keeps single-target spells on the targeting flow, plain actions plain', () => {
    expect(classifyAction(bolt)).toBe('target');
    expect(classifyAction(plain)).toBe('plain');
    expect(isMultiSelect(bolt)).toBe(false);
  });
});

describe('attackers: one optional subset slot', () => {
  it('toggles a subset and confirms with the chosen ids', () => {
    let s = beginMultiSelect(attackers);
    expect(activeSlot(s)?.slot).toBe('attackers');
    expect(activeCandidates(s)).toEqual(['atk_1', 'atk_2']);
    expect(isLastSlot(s)).toBe(true);
    // A subset slot is always satisfiable — even the empty declaration is legal.
    expect(allSlotsSatisfied(s)).toBe(true);

    s = toggle(s, 'atk_1');
    s = toggle(s, 'atk_2');
    expect(activeChosen(s)).toEqual(['atk_1', 'atk_2']);
    expect(assembleChoices(s)).toEqual([{ slot: 'attackers', chosen: ['atk_1', 'atk_2'] }]);

    // Toggling an already-chosen id removes it.
    s = toggle(s, 'atk_1');
    expect(activeChosen(s)).toEqual(['atk_2']);
  });

  it('ignores a toggle for an id the slot did not advertise', () => {
    const s = beginMultiSelect(attackers);
    expect(toggle(s, 'not_a_candidate')).toBe(s);
  });
});

describe('blockers: one subset slot per attacker (two-level pick)', () => {
  it('walks the per-attacker slots and keys each answer to its slot', () => {
    let s = beginMultiSelect(blockers);
    expect(activeSlot(s)?.slot).toBe('block_atk_1');
    expect(isLastSlot(s)).toBe(false);
    s = toggle(s, 'blk_1');
    s = toggle(s, 'blk_2');

    s = advance(s);
    expect(activeSlot(s)?.slot).toBe('block_atk_2');
    expect(isLastSlot(s)).toBe(true);
    expect(activeCandidates(s)).toEqual(['blk_1']);
    s = toggle(s, 'blk_1');

    expect(assembleChoices(s)).toEqual([
      { slot: 'block_atk_1', chosen: ['blk_1', 'blk_2'] },
      { slot: 'block_atk_2', chosen: ['blk_1'] },
    ]);
  });

  it('clamps advance at the last slot', () => {
    const s = advance(advance(advance(beginMultiSelect(blockers))));
    expect(s.active).toBe(1);
  });
});

describe('bottoming: count-bounded select_from_zone', () => {
  it('gates satisfaction on exactly count and assembles option + selection', () => {
    let s = beginMultiSelect(mulligan);
    expect(hasOptions(s)).toBe(true);
    // The walked slot is the bottoming; the option is carried separately.
    expect(s.slots.map((slot) => slot.slot)).toEqual(['bottom']);
    expect(activeSlot(s)?.kind).toBe('count');
    expect(activeSlot(s)?.count).toBe(1);

    // Nothing chosen: not exactly-count, but no partial pick either (mulligan ok).
    expect(allSlotsSatisfied(s)).toBe(false);
    expect(optionsSubmittable(s)).toBe(true);

    s = toggle(s, 'card_a');
    expect(allSlotsSatisfied(s)).toBe(true);
    expect(optionsSubmittable(s)).toBe(true);

    // Over-count is a partial/invalid pick: options are blocked until fixed.
    s = toggle(s, 'card_b');
    expect(allSlotsSatisfied(s)).toBe(false);
    expect(optionsSubmittable(s)).toBe(false);

    s = toggle(s, 'card_b');
    // Keep answers both the decision and the bottoming slot atomically.
    expect(assembleChoices(s, [{ slot: 'decision', chosen: ['keep'] }])).toEqual([
      { slot: 'decision', chosen: ['keep'] },
      { slot: 'bottom', chosen: ['card_a'] },
    ]);
  });
});
