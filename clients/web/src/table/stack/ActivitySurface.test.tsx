import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import type { GameLogEntry, GameView } from '../../protocol';
import { ACTIVITY } from './activityFeed';
import { ActivitySurface } from './ActivitySurface';

beforeEach(() => vi.useFakeTimers({ shouldAdvanceTime: true }));
afterEach(() => {
  vi.useRealTimers();
  cleanup();
});

function viewWith(log: GameLogEntry[]): GameView {
  return {
    you: 'p1',
    my_hand: [],
    me: { life: 20, library_size: 40 },
    opponents: [{ player_id: 'p2', life: 20, library_size: 40, hand_size: 7, graveyard_size: 0 }],
    battlefield: [],
    stack: [],
    graveyards: [],
    exile: [],
    phase: 'precombat_main',
    turn: 1,
    active_player: 'p1',
    seat_order: ['p1', 'p2'],
    mana_pool: [],
    valid_actions: [],
    player_names: { p1: 'Imogen', p2: 'Sorel' },
    commander_damage: [],
    log,
  };
}

function cast(sequence: number, name: string): GameLogEntry {
  return {
    sequence,
    event: { type: 'spell_cast', player: 'p2', card: { id: `c${sequence}`, name } },
  };
}

function step(sequence: number): GameLogEntry {
  return {
    sequence,
    event: { type: 'step_changed', turn: 1, phase: 'upkeep', active_player: 'p1' },
  };
}

describe('ActivitySurface — the empty log costs nothing', () => {
  it('renders no node at all with an empty log window', () => {
    const { container } = render(<ActivitySurface view={viewWith([])} />);
    expect(container.firstChild).toBeNull();
    expect(screen.queryByTestId('activity-badge')).toBeNull();
  });
});

describe('ActivitySurface — automatic surfacing', () => {
  it('surfaces a meaningful event without being asked', () => {
    render(<ActivitySurface view={viewWith([cast(1, 'Shock')])} />);
    const ticker = screen.getByTestId('activity-ticker');
    expect(ticker.textContent).toContain('cast');
    expect(ticker.getAttribute('aria-live')).toBe('polite');
  });

  it('does not surface step spam', () => {
    render(<ActivitySurface view={viewWith([step(1), step(2)])} />);
    expect(screen.queryByTestId('activity-ticker')).toBeNull();
    // The badge is still there — the history stays reachable.
    expect(screen.getByTestId('activity-badge')).toBeTruthy();
  });

  it('dwells out again, so activity never becomes a permanent column', () => {
    render(<ActivitySurface view={viewWith([cast(1, 'Shock')])} />);
    expect(screen.getByTestId('activity-ticker')).toBeTruthy();
    act(() => void vi.advanceTimersByTime(ACTIVITY.dwellMs + 1));
    expect(screen.queryByTestId('activity-ticker')).toBeNull();
    expect(screen.getByTestId('activity-badge')).toBeTruthy();
  });

  it('surfaces again when something newer arrives after the dwell', () => {
    const { rerender } = render(<ActivitySurface view={viewWith([cast(1, 'Shock')])} />);
    act(() => void vi.advanceTimersByTime(ACTIVITY.dwellMs + 1));
    expect(screen.queryByTestId('activity-ticker')).toBeNull();
    rerender(<ActivitySurface view={viewWith([cast(1, 'Shock'), cast(2, 'Bolt')])} />);
    expect(screen.getByTestId('activity-ticker').textContent).toContain('Bolt');
  });

  it('makes a surfaced reference presentationally highlightable', () => {
    const onHighlight = vi.fn();
    render(<ActivitySurface view={viewWith([cast(1, 'Shock')])} onHighlight={onHighlight} />);
    fireEvent.click(screen.getByTestId('activity-ref-c1'));
    expect(onHighlight).toHaveBeenCalledWith('c1');
  });

  it('renders references as plain text in a read-only context', () => {
    render(<ActivitySurface view={viewWith([cast(1, 'Shock')])} />);
    expect(screen.queryByTestId('activity-ref-c1')).toBeNull();
    expect(screen.getByTestId('activity-ticker').textContent).toContain('Shock');
  });
});

describe('ActivitySurface — the explicit door to the full history', () => {
  it('opens the whole log from the badge, and closes without answering anything', () => {
    render(<ActivitySurface view={viewWith([cast(1, 'Shock'), step(2)])} />);
    expect(screen.queryByTestId('activity-history')).toBeNull();
    fireEvent.click(screen.getByTestId('activity-badge'));
    const history = screen.getByTestId('activity-history');
    // The shipped log panel, composed rather than reimplemented.
    expect(within(history).getByTestId('game-log')).toBeTruthy();
    expect(within(history).getByTestId('log-entry-1')).toBeTruthy();
    fireEvent.click(screen.getByTestId('activity-history-close'));
    expect(screen.queryByTestId('activity-history')).toBeNull();
  });

  it('closes on Escape — a layer the player can dismiss without answering', () => {
    render(<ActivitySurface view={viewWith([cast(1, 'Shock')])} />);
    fireEvent.click(screen.getByTestId('activity-badge'));
    expect(screen.getByTestId('activity-history')).toBeTruthy();
    fireEvent.keyDown(window, { key: 'Escape' });
    expect(screen.queryByTestId('activity-history')).toBeNull();
  });

  it('retires the ticker while the history is open, so nothing announces twice', () => {
    render(<ActivitySurface view={viewWith([cast(1, 'Shock')])} />);
    expect(screen.getByTestId('activity-ticker')).toBeTruthy();
    fireEvent.click(screen.getByTestId('activity-badge'));
    expect(screen.queryByTestId('activity-ticker')).toBeNull();
  });

  it('carries an accessible sentence on the badge, never a bare glyph', () => {
    render(<ActivitySurface view={viewWith([cast(1, 'Shock')])} />);
    const badge = screen.getByTestId('activity-badge');
    expect(badge.getAttribute('aria-label')).toBe('Activity — 1 event. Open the full history.');
    expect(badge.getAttribute('aria-controls')).toBe('stack-activity-history');
  });
});
