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
  moveInActiveSlot,
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

/** A standalone `option` decision (issue #157): only a modal option prompt. */
const optionOnly: ValidAction = {
  id: 'a8',
  type: 'choose_mode',
  label: 'Fork in the Road',
  token: 'h:mode',
  prompts: [
    {
      kind: 'option',
      slot: 'mode',
      prompt: 'Choose a mode',
      options: [
        { id: 'draw', label: 'Draw a card' },
        { id: 'gain', label: 'Gain 3 life' },
      ],
    },
  ],
};

/** An `order` decision (issue #157): arrange three items into a permutation. */
const orderTriggers: ValidAction = {
  id: 'a9',
  type: 'order_triggers',
  label: 'Order triggers',
  token: 'h:ord0',
  prompts: [
    {
      kind: 'order',
      slot: 'order',
      prompt: 'Order these triggered abilities',
      items: ['trig_a', 'trig_b', 'trig_c'],
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

describe('option: a standalone modal decision (issue #157)', () => {
  it('carries the option as a separate slot with no walked selection slot', () => {
    const s = beginMultiSelect(optionOnly);
    expect(hasOptions(s)).toBe(true);
    expect(s.slots).toEqual([]);
    expect(activeSlot(s)).toBeNull();
    expect(s.options[0]?.options.map((o) => o.id)).toEqual(['draw', 'gain']);
    // With no count slot to gate it, the option is always submittable.
    expect(optionsSubmittable(s)).toBe(true);
    // The caller supplies the chosen option id; there are no walked-slot choices.
    expect(assembleChoices(s, [{ slot: 'mode', chosen: ['gain'] }])).toEqual([
      { slot: 'mode', chosen: ['gain'] },
    ]);
  });
});

describe('order: arrange N items into a permutation (issue #157)', () => {
  it('pre-fills the slot with the initial order and is satisfied from the start', () => {
    const s = beginMultiSelect(orderTriggers);
    expect(hasOptions(s)).toBe(false);
    expect(activeSlot(s)?.kind).toBe('order');
    expect(isLastSlot(s)).toBe(true);
    // Every item is included in the server's initial order, so it is complete.
    expect(activeChosen(s)).toEqual(['trig_a', 'trig_b', 'trig_c']);
    expect(allSlotsSatisfied(s)).toBe(true);
    expect(assembleChoices(s)).toEqual([{ slot: 'order', chosen: ['trig_a', 'trig_b', 'trig_c'] }]);
  });

  it('moves an item one step and clamps at each end', () => {
    let s = beginMultiSelect(orderTriggers);
    // Move the middle item up, then the (now) middle item down.
    s = moveInActiveSlot(s, 'trig_b', -1);
    expect(activeChosen(s)).toEqual(['trig_b', 'trig_a', 'trig_c']);
    s = moveInActiveSlot(s, 'trig_a', 1);
    expect(activeChosen(s)).toEqual(['trig_b', 'trig_c', 'trig_a']);

    // Clamped: the first item cannot move up, the last cannot move down.
    expect(moveInActiveSlot(s, 'trig_b', -1)).toBe(s);
    expect(moveInActiveSlot(s, 'trig_a', 1)).toBe(s);
    // An id not in the list is ignored.
    expect(moveInActiveSlot(s, 'not_here', -1)).toBe(s);
    // Still a full permutation — always confirmable.
    expect(allSlotsSatisfied(s)).toBe(true);
  });

  it('ignores a move on a non-order (toggle) slot', () => {
    const s = beginMultiSelect(attackers);
    expect(moveInActiveSlot(s, 'atk_1', 1)).toBe(s);
  });

  it('drives the combat-damage assignment order for a multi-blocked attacker (issue #346)', () => {
    // An `order_combat_damage` action carries one `order` prompt over an attacker's
    // blockers; the client reorders them and returns the chosen permutation, which
    // the server assigns lethal damage along. Same machinery as any order prompt.
    const orderCombatDamage: ValidAction = {
      id: 'a12',
      type: 'order_combat_damage',
      label: 'Order combat damage',
      token: 'h:dmg0',
      prompts: [
        {
          kind: 'order',
          slot: 'order_7',
          prompt: 'Order damage assignment for Thornback Boar',
          items: ['perm_11', 'perm_12'],
        },
      ],
    };
    let s = beginMultiSelect(orderCombatDamage);
    expect(activeSlot(s)?.kind).toBe('order');
    // Pre-filled with the server's battlefield order and confirmable immediately.
    expect(assembleChoices(s)).toEqual([{ slot: 'order_7', chosen: ['perm_11', 'perm_12'] }]);
    // Reorder to kill the second blocker first.
    s = moveInActiveSlot(s, 'perm_12', -1);
    expect(assembleChoices(s)).toEqual([{ slot: 'order_7', chosen: ['perm_12', 'perm_11'] }]);
    expect(allSlotsSatisfied(s)).toBe(true);
  });
});

describe('multi-select per-attacker defender flow (multiplayer, issue #347)', () => {
  /** Declare-attackers in a 3-player game: the `attackers` subset slot plus one
   * `defend_<permId>` slot per attacker candidate, each listing the defending
   * players that attacker may be assigned to (server `attacker_requirements`). */
  const splitAttackers: ValidAction = {
    id: 'a7',
    type: 'declare_attackers',
    label: 'Declare attackers',
    token: 'h:atk1',
    requirements: [
      { slot: 'attackers', prompt: 'Choose attackers', candidates: ['perm_1', 'perm_2'] },
      { slot: 'defend_1', prompt: 'Choose whom Rhino attacks', candidates: ['p2', 'p3'] },
      { slot: 'defend_2', prompt: 'Choose whom Falcon attacks', candidates: ['p2', 'p3'] },
    ],
  };

  it('classifies the defender slots and links each to its attacker', () => {
    const s = beginMultiSelect(splitAttackers);
    expect(s.slots.map((slot) => slot.kind)).toEqual(['subset', 'defender', 'defender']);
    expect(s.slots[1]?.attacker).toBe('perm_1');
    expect(s.slots[2]?.attacker).toBe('perm_2');
  });

  it('walks only the declared attackers’ defender slots', () => {
    // Declare only the first attacker; advancing skips the second's defender slot.
    let s = beginMultiSelect(splitAttackers);
    s = toggle(s, 'perm_1');
    expect(isLastSlot(s)).toBe(false); // defend_1 is still to walk
    s = advance(s);
    expect(activeSlot(s)?.slot).toBe('defend_1');
    expect(isLastSlot(s)).toBe(true); // defend_2 is skipped (perm_2 not attacking)
  });

  it('assigns two attackers to different defenders and submits atomically', () => {
    let s = beginMultiSelect(splitAttackers);
    // Declare both attackers.
    s = toggle(s, 'perm_1');
    s = toggle(s, 'perm_2');
    // Not submittable yet: each declared attacker owes a defender.
    expect(allSlotsSatisfied(s)).toBe(false);
    // Assign the rhino to p2…
    s = advance(s);
    expect(activeSlot(s)?.slot).toBe('defend_1');
    s = toggle(s, 'p2');
    expect(allSlotsSatisfied(s)).toBe(false);
    // …and the falcon to p3.
    s = advance(s);
    expect(activeSlot(s)?.slot).toBe('defend_2');
    s = toggle(s, 'p3');
    expect(allSlotsSatisfied(s)).toBe(true);
    expect(isLastSlot(s)).toBe(true);
    // One atomic answer: the attacker subset plus each attacker's defender.
    expect(assembleChoices(s)).toEqual([
      { slot: 'attackers', chosen: ['perm_1', 'perm_2'] },
      { slot: 'defend_1', chosen: ['p2'] },
      { slot: 'defend_2', chosen: ['p3'] },
    ]);
  });

  it('treats a defender pick as single-select (replaces, never accumulates)', () => {
    let s = beginMultiSelect(splitAttackers);
    s = toggle(s, 'perm_1');
    s = advance(s);
    s = toggle(s, 'p2');
    s = toggle(s, 'p3'); // change of mind
    expect(activeChosen(s)).toEqual(['p3']);
  });

  it('omits an undeclared attacker’s defender slot from the answer', () => {
    let s = beginMultiSelect(splitAttackers);
    s = toggle(s, 'perm_1');
    s = advance(s);
    s = toggle(s, 'p2');
    // Only perm_1 was declared, so only its defender slot is submitted.
    expect(assembleChoices(s)).toEqual([
      { slot: 'attackers', chosen: ['perm_1'] },
      { slot: 'defend_1', chosen: ['p2'] },
    ]);
    expect(activeCandidates(s)).toEqual(['p2', 'p3']);
  });

  it('lets the player declare no attackers with no defender step (empty is legal)', () => {
    const s = beginMultiSelect(splitAttackers);
    // Nothing toggled: no defender slot is in play, so it is immediately confirmable.
    expect(allSlotsSatisfied(s)).toBe(true);
    expect(isLastSlot(s)).toBe(true);
    expect(assembleChoices(s)).toEqual([{ slot: 'attackers', chosen: [] }]);
  });

  it('is unchanged for a two-player declare (no defender slots)', () => {
    // A two-player declare-attackers carries only the `attackers` slot — the fast path
    // gains no extra step (server omits defend_* with a sole opponent).
    const twoPlayer: ValidAction = {
      id: 'a8',
      type: 'declare_attackers',
      label: 'Declare attackers',
      token: 'h:atk2',
      requirements: [{ slot: 'attackers', prompt: 'Choose attackers', candidates: ['perm_1'] }],
    };
    let s = beginMultiSelect(twoPlayer);
    expect(s.slots.map((slot) => slot.kind)).toEqual(['subset']);
    s = toggle(s, 'perm_1');
    expect(isLastSlot(s)).toBe(true);
    expect(assembleChoices(s)).toEqual([{ slot: 'attackers', chosen: ['perm_1'] }]);
  });
});
