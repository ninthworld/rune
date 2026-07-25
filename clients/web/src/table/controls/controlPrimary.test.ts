/**
 * The §4.2 derivation table, row by row (`docs/design/control-language.md` §4,
 * issue #534).
 *
 * One test per rule, in the document's order, plus the two tie rows and the
 * `concede` exclusion, because those three are the cases a well-meaning refactor
 * "fixes": a tie looks like a missing feature, and `concede` looks like just
 * another subject-less action. They are neither — see D8 and D9.
 */
import { describe, expect, it } from 'vitest';
import type { ValidAction } from '../../protocol';
import { derivePrimary } from './controlPrimary';

/** A `ValidAction` with only the fields §4.2 is allowed to read. */
function action(id: string, type: string, label: string, subject: string[] = []): ValidAction {
  return { id, type, label, subject };
}

const pass = action('a-pass', 'pass_priority', 'PASS PRIORITY');
const concede = action('a-concede', 'concede', 'CONCEDE');
const castBolt = action('a-cast', 'cast_spell', 'CAST SPELL', ['c1']);
const cycleBolt = action('a-cycle', 'activate_ability', 'CYCLE', ['c1']);

describe('primary derivation (§4.2)', () => {
  it('rule 1 — a multi-select session empties the slot; the plaque confirms', () => {
    const derived = derivePrimary({ validActions: [pass], session: 'multiSelect' });
    expect(derived.rule).toBe(1);
    expect(derived.primary).toBeUndefined();
    // Not "waiting": actions exist, the decision plaque simply owns the advance.
    expect(derived.waiting).toBe(false);
  });

  it('rule 2 — a targeting session empties the slot; the last pick submits', () => {
    const derived = derivePrimary({ validActions: [pass], session: 'targeting' });
    expect(derived.rule).toBe(2);
    expect(derived.primary).toBeUndefined();
  });

  it('rule 3 — one selected entity with exactly one action promotes that action', () => {
    const derived = derivePrimary({ validActions: [pass, castBolt], selectedId: 'c1' });
    expect(derived.rule).toBe(3);
    expect(derived.primary).toBe(castBolt);
    // The label is the server's, verbatim — the client names no action.
    expect(derived.primary?.label).toBe('CAST SPELL');
    expect(derived.secondaries).toEqual([]);
  });

  it('rule 3 outranks rule 5: a selection beats the offered pass', () => {
    // The order of the table is load-bearing. With a card selected, the brightest
    // control must be the player's declared intent, not the global pass.
    const derived = derivePrimary({ validActions: [pass, castBolt], selectedId: 'c1' });
    expect(derived.primary).not.toBe(pass);
  });

  it('rule 4 — a selection with two actions leaves the slot EMPTY (a tie)', () => {
    // D8: choosing among several offered actions would be a client judgement.
    const derived = derivePrimary({
      validActions: [pass, castBolt, cycleBolt],
      selectedId: 'c1',
    });
    expect(derived.rule).toBe(4);
    expect(derived.primary).toBeUndefined();
    expect(derived.secondaries).toEqual([castBolt, cycleBolt]);
  });

  it('rule 5 — no selection and a pass on offer promotes the pass', () => {
    const other = action('a-attack', 'declare_attackers', 'CONFIRM ATTACKERS');
    const derived = derivePrimary({ validActions: [other, pass] });
    expect(derived.rule).toBe(5);
    expect(derived.primary).toBe(pass);
    // Everything else subject-less still renders, flat, in the echo.
    expect(derived.secondaries).toEqual([other]);
  });

  it('rule 5 renders the server label verbatim, never a client "RESOLVE"', () => {
    // GAP-2: deciding that a pass resolves the stack top is a rules judgement the
    // client may not make, so the derivation reports the action, not a string.
    const derived = derivePrimary({ validActions: [pass], stackDepth: 2 });
    expect(derived.primary?.label).toBe('PASS PRIORITY');
  });

  it('rule 6 — exactly one subject-less entry, and no pass, promotes it', () => {
    const confirm = action('a-attack', 'declare_attackers', 'CONFIRM ATTACKERS');
    const derived = derivePrimary({ validActions: [confirm, castBolt] });
    expect(derived.rule).toBe(6);
    expect(derived.primary).toBe(confirm);
    expect(derived.secondaries).toEqual([]);
  });

  it('rule 7 — two subject-less entries leave the slot EMPTY (the second tie)', () => {
    const a = action('a-1', 'declare_attackers', 'CONFIRM ATTACKERS');
    const b = action('a-2', 'declare_blockers', 'CONFIRM BLOCKERS');
    const derived = derivePrimary({ validActions: [a, b] });
    expect(derived.rule).toBe(7);
    expect(derived.primary).toBeUndefined();
    expect(derived.secondaries).toEqual([a, b]);
  });

  it('rule 8 — an empty valid_actions is the "Waiting" state', () => {
    const derived = derivePrimary({ validActions: [] });
    expect(derived.rule).toBe(8);
    expect(derived.primary).toBeUndefined();
    expect(derived.secondaries).toEqual([]);
    expect(derived.waiting).toBe(true);
  });

  it('never claims "Waiting" while any action is offered', () => {
    // The plaque's "Waiting" is rule 8 alone. Entity actions with no selection
    // match no row (rule 0) — the slot empties, but the player is not idle.
    const derived = derivePrimary({ validActions: [castBolt] });
    expect(derived.rule).toBe(0);
    expect(derived.primary).toBeUndefined();
    expect(derived.waiting).toBe(false);
  });

  it('never promotes concede, and never echoes it either (D9)', () => {
    // Alone, concede would otherwise satisfy rule 6 exactly.
    const soloDerived = derivePrimary({ validActions: [concede] });
    expect(soloDerived.rule).toBe(0);
    expect(soloDerived.primary).toBeUndefined();
    expect(soloDerived.secondaries).toEqual([]);

    // Beside one real action it must not turn rule 6 into rule 7's tie.
    const confirm = action('a-attack', 'declare_attackers', 'CONFIRM ATTACKERS');
    const paired = derivePrimary({ validActions: [concede, confirm] });
    expect(paired.rule).toBe(6);
    expect(paired.primary).toBe(confirm);
    expect(paired.secondaries).toEqual([]);
  });
});

describe('RESPOND and the form switch (§4.3, §4.4)', () => {
  it('offers RESPOND only when the primary is pass AND the stack is non-empty', () => {
    expect(derivePrimary({ validActions: [pass], stackDepth: 1 }).respond).toBe(true);
    expect(derivePrimary({ validActions: [pass], stackDepth: 0 }).respond).toBe(false);
  });

  it('never offers RESPOND beside a non-pass primary', () => {
    // There is nothing to "respond instead of" when the primary casts a spell.
    const derived = derivePrimary({
      validActions: [castBolt],
      selectedId: 'c1',
      stackDepth: 3,
    });
    expect(derived.primary).toBe(castBolt);
    expect(derived.respond).toBe(false);
  });

  it('switches the primary to the compact form on the stack alone (D7)', () => {
    expect(derivePrimary({ validActions: [pass], stackDepth: 0 }).form).toBe('stadium');
    expect(derivePrimary({ validActions: [pass], stackDepth: 2 }).form).toBe('compact');
    // Independent of which rule won: the pair must read against the rail
    // whatever the primary says.
    expect(derivePrimary({ validActions: [castBolt], selectedId: 'c1', stackDepth: 2 }).form).toBe(
      'compact',
    );
  });
});

describe('what the derivation is allowed to look at', () => {
  it('returns entries of the input list by identity, never a copy', () => {
    // The cluster can then only ever echo back an id the server issued: there is
    // no path by which a constructed action reaches `ChooseAction`.
    const derived = derivePrimary({ validActions: [pass] });
    expect(derived.primary).toBe(pass);
  });

  it('treats an absent subject as an empty one (the wire convention)', () => {
    const bare: ValidAction = { id: 'a-bare', type: 'declare_attackers', label: 'GO' };
    const derived = derivePrimary({ validActions: [bare] });
    expect(derived.rule).toBe(6);
    expect(derived.primary).toBe(bare);
  });
});
