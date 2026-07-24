import { beforeEach, describe, expect, it, vi } from 'vitest';
import { act, fireEvent, render, screen, within } from '@testing-library/react';
import {
  BOTTOM_GAME_VIEW_JSON,
  DECLARE_ATTACKERS_GAME_VIEW_JSON,
  DECLARE_BLOCKERS_GAME_VIEW_JSON,
  MULLIGAN_GAME_VIEW_JSON,
  SAMPLE_GAME_VIEW_JSON,
} from '../game-view.fixture';
import type { TargetChoice, ValidAction } from '../protocol';
import { useGameStore } from '../store';
import { Table } from './Table';
import { registerTableTestHooks, seed } from './table-test-support';

registerTableTestHooks();

describe('Table multi-select: declare attackers (issue #143)', () => {
  let choose: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    choose = seed(DECLARE_ATTACKERS_GAME_VIEW_JSON);
    render(<Table />);
  });

  /** Open the attackers declaration from its subject-less action-bar button. */
  function enter(): void {
    fireEvent.click(
      within(screen.getByTestId('action-bar')).getByRole('button', { name: 'Declare attackers' }),
    );
  }

  it('opens multi-select (not a submit) and highlights exactly the candidates', () => {
    enter();
    expect(choose).not.toHaveBeenCalled();
    expect(screen.getByTestId('multiselect-prompt').textContent).toContain('Choose attackers');
    // Both eligible attackers are toggleable; nothing else is.
    expect(screen.getByTestId('target-atk_1')).toBeDefined();
    expect(screen.getByTestId('target-atk_2')).toBeDefined();
    expect(screen.queryByTestId('entity-atk_1')).toBeNull();
  });

  it('toggles a subset and confirms it atomically with the token', () => {
    enter();
    fireEvent.click(screen.getByTestId('target-atk_1'));
    fireEvent.click(screen.getByTestId('target-atk_2'));
    // Toggling reflects a running count in the banner.
    expect(screen.getByTestId('multiselect-count').textContent).toContain('2 selected');

    fireEvent.click(screen.getByTestId('multiselect-confirm'));
    expect(choose).toHaveBeenCalledTimes(1);
    const [action, targets] = choose.mock.calls[0] as [ValidAction, TargetChoice[]];
    expect(action.id).toBe('a5');
    expect(action.token).toBe('h:atk0');
    expect(targets).toEqual([{ slot: 'attackers', chosen: ['atk_1', 'atk_2'] }]);
  });

  it('allows the empty declaration (confirm with no attackers)', () => {
    enter();
    // Subset slots are always confirmable — the empty set legally declares none.
    fireEvent.click(screen.getByTestId('multiselect-confirm'));
    const [, targets] = choose.mock.calls[0] as [ValidAction, TargetChoice[]];
    expect(targets).toEqual([{ slot: 'attackers', chosen: [] }]);
  });

  it('untoggles a candidate on a second click', () => {
    enter();
    fireEvent.click(screen.getByTestId('target-atk_1'));
    fireEvent.click(screen.getByTestId('target-atk_1'));
    fireEvent.click(screen.getByTestId('multiselect-confirm'));
    const [, targets] = choose.mock.calls[0] as [ValidAction, TargetChoice[]];
    expect(targets).toEqual([{ slot: 'attackers', chosen: [] }]);
  });

  it('cancels without submitting, restoring the neutral action bar', () => {
    enter();
    fireEvent.click(screen.getByTestId('multiselect-cancel'));
    expect(choose).not.toHaveBeenCalled();
    expect(screen.queryByTestId('target-atk_1')).toBeNull();
    expect(
      within(screen.getByTestId('action-bar')).getByRole('button', { name: 'Declare attackers' }),
    ).toBeDefined();
  });

  it('drops the in-progress selection when a fresh view arrives (changed token)', () => {
    enter();
    fireEvent.click(screen.getByTestId('target-atk_1'));
    act(() => useGameStore.getState().ingest(SAMPLE_GAME_VIEW_JSON));
    expect(screen.queryByTestId('target-atk_1')).toBeNull();
    expect(screen.queryByTestId('multiselect-prompt')).toBeNull();
    expect(choose).not.toHaveBeenCalled();
  });
});

describe('Table multi-select: declare blockers per-attacker (issue #143)', () => {
  let choose: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    choose = seed(DECLARE_BLOCKERS_GAME_VIEW_JSON);
    render(<Table />);
  });

  function enter(): void {
    fireEvent.click(
      within(screen.getByTestId('action-bar')).getByRole('button', { name: 'Declare blockers' }),
    );
  }

  it('walks one slot per attacker and assigns blockers per attacker', () => {
    enter();
    // First attacker's slot: both defenders are eligible to block it.
    expect(screen.getByTestId('multiselect-prompt').textContent).toContain('Verdant Scout');
    expect(screen.getByTestId('multiselect-step').textContent).toContain('Step 1 of 2');
    fireEvent.click(screen.getByTestId('target-blk_1'));
    fireEvent.click(screen.getByTestId('target-blk_2'));

    // Advance to the second attacker's slot; only one defender may block it.
    fireEvent.click(within(screen.getByTestId('action-bar')).getByRole('button', { name: 'Next' }));
    expect(screen.getByTestId('multiselect-prompt').textContent).toContain('Hill Giant');
    expect(screen.queryByTestId('target-blk_2')).toBeNull();
    fireEvent.click(screen.getByTestId('target-blk_1'));

    fireEvent.click(screen.getByTestId('multiselect-confirm'));
    const [action, targets] = choose.mock.calls[0] as [ValidAction, TargetChoice[]];
    expect(action.token).toBe('h:blk0');
    expect(targets).toEqual([
      { slot: 'block_atk_1', chosen: ['blk_1', 'blk_2'] },
      { slot: 'block_atk_2', chosen: ['blk_1'] },
    ]);
  });
});

describe('Table multi-select: mulligan bottoming (issue #143)', () => {
  let choose: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    choose = seed(MULLIGAN_GAME_VIEW_JSON);
    render(<Table />);
  });

  function enter(): void {
    fireEvent.click(
      within(screen.getByTestId('action-bar')).getByRole('button', { name: 'Keep or mulligan' }),
    );
  }

  it('renders the keep/mulligan options and the hand bottoming candidates', () => {
    enter();
    expect(screen.getByTestId('multiselect-option-keep')).toBeDefined();
    expect(screen.getByTestId('multiselect-option-mulligan')).toBeDefined();
    // The select_from_zone candidates are the hand cards.
    expect(screen.getByTestId('target-card_a')).toBeDefined();
    expect(screen.getByTestId('target-card_b')).toBeDefined();
    expect(screen.getByTestId('multiselect-count').textContent).toContain('0 of 1 selected');
  });

  it('keeps and bottoms the picked card in one atomic answer', () => {
    enter();
    fireEvent.click(screen.getByTestId('target-card_a'));
    fireEvent.click(screen.getByTestId('multiselect-option-keep'));
    expect(choose).toHaveBeenCalledTimes(1);
    const [action, targets] = choose.mock.calls[0] as [ValidAction, TargetChoice[]];
    expect(action.token).toBe('h:mull');
    expect(targets).toEqual([
      { slot: 'decision', chosen: ['keep'] },
      { slot: 'bottom', chosen: ['card_a'] },
    ]);
  });

  it('blocks the option buttons while the bottom pick exceeds the advertised count', () => {
    enter();
    // count is 1: picking a second card makes the selection invalid, disabling submit.
    fireEvent.click(screen.getByTestId('target-card_a'));
    fireEvent.click(screen.getByTestId('target-card_b'));
    expect(screen.getByTestId('multiselect-option-keep')).toHaveProperty('disabled', true);
    expect(screen.getByTestId('multiselect-option-mulligan')).toHaveProperty('disabled', true);
  });
});

describe('Table multi-select: select-from-zone count gate (issue #143)', () => {
  let choose: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    choose = seed(BOTTOM_GAME_VIEW_JSON);
    render(<Table />);
  });

  function enter(): void {
    fireEvent.click(
      within(screen.getByTestId('action-bar')).getByRole('button', { name: 'Keep hand' }),
    );
  }

  it('disables confirm until exactly the advertised count is picked', () => {
    enter();
    // Nothing picked: confirm is disabled (count is 2). This is a UX affordance only.
    expect(screen.getByTestId('multiselect-confirm')).toHaveProperty('disabled', true);
    fireEvent.click(screen.getByTestId('target-card_a'));
    expect(screen.getByTestId('multiselect-confirm')).toHaveProperty('disabled', true);

    fireEvent.click(screen.getByTestId('target-card_b'));
    expect(screen.getByTestId('multiselect-confirm')).toHaveProperty('disabled', false);

    fireEvent.click(screen.getByTestId('multiselect-confirm'));
    const [action, targets] = choose.mock.calls[0] as [ValidAction, TargetChoice[]];
    expect(action.token).toBe('h:keep');
    expect(targets).toEqual([{ slot: 'bottom', chosen: ['card_a', 'card_b'] }]);
  });
});
