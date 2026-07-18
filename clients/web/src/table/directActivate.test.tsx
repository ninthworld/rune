/**
 * Direct entity activation (ADR 0025): the one-gesture vocabulary that
 * shortcuts select→dock where intent is unambiguous —
 *
 * 1. a combat-declaration candidate enters the declaration pre-toggled,
 * 2. a sole server-flagged mana ability fires on the first activation,
 * 3. the already-selected entity's sole action fires on the second activation.
 *
 * All three ride the entity's single click/tap/keyboard-activate handler, so
 * the tests drive plain clicks on the real <Table />.
 */
import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { Table } from './Table';
import { useGameStore } from '../store';

afterEach(cleanup);

/** A battlefield with an untapped Forest whose flagged mana ability is offered. */
const MANA_VIEW_JSON = JSON.stringify({
  you: 'p1',
  my_hand: [],
  opponents: [{ player_id: 'p2', hand_size: 0, life: 20, library_size: 40 }],
  battlefield: [
    {
      id: 'perm_f',
      controller: 'p1',
      owner: 'p1',
      card: { id: 'c_f', name: 'Forest', type_line: 'Basic Land — Forest' },
    },
  ],
  phase: 'precombat_main',
  valid_actions: [
    { id: 'a1', type: 'pass_priority', label: 'Pass', token: 'h:pass' },
    {
      id: 'a2',
      type: 'activate_ability',
      label: '{T}: Add {G}.',
      subject: ['perm_f'],
      mana_ability: true,
      token: 'h:tap',
    },
  ],
});

/** The same permanent offering a sole NON-mana activation (no flag). */
const ABILITY_VIEW_JSON = JSON.stringify({
  you: 'p1',
  my_hand: [],
  opponents: [{ player_id: 'p2', hand_size: 0, life: 20, library_size: 40 }],
  battlefield: [
    {
      id: 'perm_t',
      controller: 'p1',
      owner: 'p1',
      card: { id: 'c_t', name: 'Toolbox', type_line: 'Artifact' },
    },
  ],
  phase: 'precombat_main',
  valid_actions: [
    { id: 'a1', type: 'pass_priority', label: 'Pass', token: 'h:pass' },
    {
      id: 'a3',
      type: 'activate_ability',
      label: '{T}: Draw a card.',
      subject: ['perm_t'],
      token: 'h:draw',
    },
  ],
});

/** A declare-attackers step: the bear is a candidate, not a subject. */
const ATTACK_VIEW_JSON = JSON.stringify({
  you: 'p1',
  my_hand: [],
  opponents: [{ player_id: 'p2', hand_size: 0, life: 20, library_size: 40 }],
  battlefield: [
    {
      id: 'perm_b',
      controller: 'p1',
      owner: 'p1',
      card: {
        id: 'c_b',
        name: 'Bear',
        type_line: 'Creature — Bear',
        power: '2',
        toughness: '2',
      },
    },
  ],
  phase: 'declare_attackers',
  valid_actions: [
    {
      id: 'a4',
      type: 'declare_attackers',
      label: 'Declare attackers',
      requirements: [{ slot: 'attackers', prompt: 'Choose attackers', candidates: ['perm_b'] }],
      token: 'h:attack',
    },
  ],
});

function seed(json: string): ReturnType<typeof vi.fn> {
  const choose = vi.fn();
  useGameStore.getState().ingest(json);
  useGameStore.setState({ choose });
  return choose;
}

describe('direct entity activation (ADR 0025)', () => {
  it('fires a sole flagged mana ability on the FIRST activation — tap the land, get the mana', () => {
    const choose = seed(MANA_VIEW_JSON);
    render(<Table />);
    fireEvent.click(screen.getByTestId('entity-perm_f'));
    expect(choose).toHaveBeenCalledTimes(1);
    expect(choose.mock.calls[0]![0].id).toBe('a2');
  });

  it('keeps select-then-act for an unflagged sole action: first selects, second fires', () => {
    const choose = seed(ABILITY_VIEW_JSON);
    render(<Table />);
    const entity = screen.getByTestId('entity-perm_t');
    fireEvent.click(entity);
    // First activation selects only — the dock shows the labeled action.
    expect(choose).not.toHaveBeenCalled();
    expect(entity.getAttribute('aria-pressed')).toBe('true');
    fireEvent.click(entity);
    expect(choose).toHaveBeenCalledTimes(1);
    expect(choose.mock.calls[0]![0].id).toBe('a3');
  });

  it('enters the attacker declaration from the creature itself, pre-toggled', () => {
    const choose = seed(ATTACK_VIEW_JSON);
    render(<Table />);
    // The candidate is directly interactive even though it carries no subject
    // action — one click opens the declaration with the bear already toggled.
    fireEvent.click(screen.getByTestId('entity-perm_b'));
    expect(choose).not.toHaveBeenCalled();
    const candidate = screen.getByTestId('target-perm_b');
    expect(candidate.getAttribute('aria-pressed')).toBe('true');
    // A second click toggles it back out — the declaration is reversible.
    fireEvent.click(candidate);
    expect(screen.getByTestId('target-perm_b').getAttribute('aria-pressed')).toBe('false');
  });

  it('submits the entered declaration atomically via Confirm', () => {
    const choose = seed(ATTACK_VIEW_JSON);
    render(<Table />);
    fireEvent.click(screen.getByTestId('entity-perm_b'));
    fireEvent.click(screen.getByText('Confirm'));
    expect(choose).toHaveBeenCalledTimes(1);
    const [action, targets] = choose.mock.calls[0]!;
    expect(action.id).toBe('a4');
    expect(targets).toEqual([{ slot: 'attackers', chosen: ['perm_b'] }]);
  });
});
