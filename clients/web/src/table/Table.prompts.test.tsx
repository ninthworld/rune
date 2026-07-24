import { beforeEach, describe, expect, it, vi } from 'vitest';
import { act, fireEvent, render, screen, within } from '@testing-library/react';
import {
  DISCARD_GAME_VIEW_JSON,
  OPTION_GAME_VIEW_JSON,
  ORDER_GAME_VIEW_JSON,
  ZONE_SELECT_GAME_VIEW_JSON,
} from '../game-view.fixture';
import type { TargetChoice, ValidAction } from '../protocol';
import { useGameStore } from '../store';
import { Table } from './Table';
import { registerTableTestHooks, seed } from './table-test-support';

registerTableTestHooks();

describe('Table option: modal picker in the banner (issue #157)', () => {
  let choose: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    choose = seed(OPTION_GAME_VIEW_JSON);
    render(<Table />);
  });

  function enter(): void {
    fireEvent.click(
      within(screen.getByTestId('action-bar')).getByRole('button', { name: 'Fork in the Road' }),
    );
  }

  it('renders the named choices as buttons in the decision sheet', () => {
    enter();
    expect(choose).not.toHaveBeenCalled();
    const sheet = screen.getByTestId('decision-sheet');
    // The modal option picker lives in the decision sheet (issue #157, restaged by
    // ADR 0023), not the action bar.
    expect(within(sheet).getByTestId('multiselect-option-draw')).toBeDefined();
    expect(within(sheet).getByTestId('multiselect-option-gain')).toBeDefined();
    expect(screen.getByTestId('multiselect-options').textContent).toContain('Choose a mode');
    // A pure option decision shows no selection count.
    expect(screen.queryByTestId('multiselect-count')).toBeNull();
  });

  it('submits the chosen option id atomically with the content-binding token', () => {
    enter();
    fireEvent.click(screen.getByTestId('multiselect-option-gain'));
    expect(choose).toHaveBeenCalledTimes(1);
    const [action, targets] = choose.mock.calls[0] as [ValidAction, TargetChoice[]];
    expect(action.id).toBe('a8');
    expect(action.token).toBe('h:mode');
    expect(targets).toEqual([{ slot: 'mode', chosen: ['gain'] }]);
  });
});

describe('Table order: arrange list (issue #157)', () => {
  let choose: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    choose = seed(ORDER_GAME_VIEW_JSON);
    render(<Table />);
  });

  function enter(): void {
    fireEvent.click(
      within(screen.getByTestId('action-bar')).getByRole('button', { name: 'Order triggers' }),
    );
  }

  it('opens the reorder surface with every item in the server order', () => {
    enter();
    expect(choose).not.toHaveBeenCalled();
    const surface = screen.getByTestId('prompt-surface');
    // Each ordered item is labelled by its card name.
    expect(within(surface).getByText('Soul Warden')).toBeDefined();
    expect(within(surface).getByText('Ajani’s Welcome')).toBeDefined();
    expect(within(surface).getByText('Impassioned Orator')).toBeDefined();
    // The first item cannot move up; the last cannot move down (clamped controls).
    expect(screen.getByTestId('order-up-trig_a')).toHaveProperty('disabled', true);
    expect(screen.getByTestId('order-down-trig_c')).toHaveProperty('disabled', true);
    // Order is always a complete permutation, so confirm is enabled immediately.
    expect(screen.getByTestId('multiselect-confirm')).toHaveProperty('disabled', false);
  });

  it('reorders items and submits the permutation with the token', () => {
    enter();
    // Move the last item (Impassioned Orator) up one: a,b,c → a,c,b.
    fireEvent.click(screen.getByTestId('order-up-trig_c'));
    fireEvent.click(screen.getByTestId('multiselect-confirm'));

    expect(choose).toHaveBeenCalledTimes(1);
    const [action, targets] = choose.mock.calls[0] as [ValidAction, TargetChoice[]];
    expect(action.token).toBe('h:ord0');
    expect(targets).toEqual([{ slot: 'order', chosen: ['trig_a', 'trig_c', 'trig_b'] }]);
  });

  it('reconstructs the identical order surface from a replayed view (rehydration)', () => {
    enter();
    // Reorder mid-prompt, then replay the same view (a refresh/reconnect resend).
    fireEvent.click(screen.getByTestId('order-up-trig_c'));
    act(() => useGameStore.getState().ingest(ORDER_GAME_VIEW_JSON));

    // The ephemeral session is dropped (no state across messages); the surface is
    // gone and the action is offered again, so the prompt is fully reconstructable.
    expect(screen.queryByTestId('prompt-surface')).toBeNull();
    expect(choose).not.toHaveBeenCalled();

    // Re-opening rebuilds the identical surface in the server's initial order —
    // the earlier reorder left no residue.
    enter();
    fireEvent.click(screen.getByTestId('multiselect-confirm'));
    const [, targets] = choose.mock.calls[0] as [ValidAction, TargetChoice[]];
    expect(targets).toEqual([{ slot: 'order', chosen: ['trig_a', 'trig_b', 'trig_c'] }]);
  });
});

describe('Table select-from-zone: non-visible zone overlay (issue #157)', () => {
  let choose: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    choose = seed(ZONE_SELECT_GAME_VIEW_JSON);
    render(<Table />);
  });

  function enter(): void {
    fireEvent.click(
      within(screen.getByTestId('action-bar')).getByRole('button', {
        name: 'Return a card to hand',
      }),
    );
  }

  it('surfaces graveyard candidates in the overlay list, not on the canvas', () => {
    enter();
    const surface = screen.getByTestId('prompt-surface');
    expect(within(surface).getByText('Llanowar Elves')).toBeDefined();
    expect(within(surface).getByTestId('zone-select-gy_2')).toBeDefined();
    // The graveyard is not on the board, so there is no canvas target hotspot.
    expect(screen.queryByTestId('target-gy_2')).toBeNull();
  });

  it('count-gates confirm and submits the picked id atomically with the token', () => {
    enter();
    expect(screen.getByTestId('multiselect-confirm')).toHaveProperty('disabled', true);
    fireEvent.click(screen.getByTestId('zone-select-gy_2'));
    expect(screen.getByTestId('multiselect-confirm')).toHaveProperty('disabled', false);

    fireEvent.click(screen.getByTestId('multiselect-confirm'));
    const [action, targets] = choose.mock.calls[0] as [ValidAction, TargetChoice[]];
    expect(action.token).toBe('h:gy0');
    expect(targets).toEqual([{ slot: 'return', chosen: ['gy_2'] }]);
  });
});

describe('Table discard-to-max end to end (issue #156/#157)', () => {
  let choose: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    choose = seed(DISCARD_GAME_VIEW_JSON);
    render(<Table />);
  });

  function enter(): void {
    fireEvent.click(
      within(screen.getByTestId('action-bar')).getByRole('button', {
        name: 'Discard to hand size',
      }),
    );
  }

  it('highlights hand cards in place (not the overlay) and submits the discard', () => {
    enter();
    // The hand IS on the board, so candidates highlight in place — no overlay list.
    expect(screen.queryByTestId('prompt-surface')).toBeNull();
    expect(screen.getByTestId('target-h8')).toBeDefined();
    expect(screen.getByTestId('multiselect-count').textContent).toContain('0 of 1 selected');

    // Confirm is count-gated: the 8th card must be chosen to complete cleanup.
    expect(screen.getByTestId('multiselect-confirm')).toHaveProperty('disabled', true);
    fireEvent.click(screen.getByTestId('target-h8'));
    expect(screen.getByTestId('multiselect-confirm')).toHaveProperty('disabled', false);

    fireEvent.click(screen.getByTestId('multiselect-confirm'));
    const [action, targets] = choose.mock.calls[0] as [ValidAction, TargetChoice[]];
    expect(action.token).toBe('h:disc');
    expect(targets).toEqual([{ slot: 'discard', chosen: ['h8'] }]);
  });

  it('cancels the discard with Escape (keyboard parity with targeting)', () => {
    enter();
    expect(screen.getByTestId('multiselect-prompt')).toBeDefined();
    fireEvent.keyDown(window, { key: 'Escape' });
    // The selection is abandoned with nothing submitted; the neutral bar returns.
    expect(choose).not.toHaveBeenCalled();
    expect(screen.queryByTestId('multiselect-prompt')).toBeNull();
    expect(
      within(screen.getByTestId('action-bar')).getByRole('button', {
        name: 'Discard to hand size',
      }),
    ).toBeDefined();
  });
});
