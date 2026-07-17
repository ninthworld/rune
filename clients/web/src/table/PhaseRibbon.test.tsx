import { afterEach, describe, expect, it } from 'vitest';
import { cleanup, render, screen, within } from '@testing-library/react';
import type { GameView, Phase } from '../protocol';
import { PhaseRibbon } from './PhaseRibbon';

afterEach(cleanup);

/** A minimal live view carrying just the fields the ribbon reads. */
function viewWith(turn: number, activePlayer: string, phase: Phase): GameView {
  return {
    you: 'p1',
    my_hand: [],
    me: { life: 20, library_size: 40 },
    opponents: [],
    battlefield: [],
    stack: [],
    graveyards: [],
    exile: [],
    phase,
    turn,
    active_player: activePlayer,
    mana_pool: [],
    valid_actions: [],
    player_names: {},
  };
}

describe('PhaseRibbon (issue #267)', () => {
  it('shows the turn number, active player, and marks the current step', () => {
    render(
      <PhaseRibbon view={viewWith(4, 'p2', 'declare_attackers')} mode="overview" localId="p1" />,
    );
    expect(screen.getByTestId('ribbon-turn').textContent).toBe('Turn 4');
    // p2's turn from p1's seat reads as the opponent's turn, not "Your turn".
    expect(screen.getByTestId('ribbon-active').textContent).toBe("p2's turn");
    // Exactly the current step is marked (aria-current + data-current).
    const current = screen.getByTestId('ribbon-step-declare_attackers');
    expect(current.getAttribute('aria-current')).toBe('step');
    expect(screen.getByTestId('ribbon-step-upkeep').getAttribute('aria-current')).toBeNull();
    // The full phase sequence is present (12 steps).
    expect(within(screen.getByTestId('ribbon-steps')).getAllByRole('listitem')).toHaveLength(12);
  });

  it('phrases the receiver’s own turn as "Your turn"', () => {
    render(<PhaseRibbon view={viewWith(1, 'p1', 'upkeep')} mode="overview" localId="p1" />);
    expect(screen.getByTestId('ribbon-active').textContent).toBe('Your turn');
  });

  it('labels the active opponent by display name when the server sent one (issue #294)', () => {
    const view = { ...viewWith(4, 'p2', 'draw'), player_names: { p2: 'Bob' } };
    render(<PhaseRibbon view={view} mode="overview" localId="p1" />);
    expect(screen.getByTestId('ribbon-active').textContent).toBe("Bob's turn");
  });

  it('shows a decision badge in focus mode and none in overview', () => {
    const view = viewWith(2, 'p1', 'precombat_main');
    const { rerender } = render(<PhaseRibbon view={view} mode="focus" localId="p1" />);
    expect(screen.getByTestId('phase-ribbon').getAttribute('data-mode')).toBe('focus');
    expect(screen.getByTestId('ribbon-focus')).toBeDefined();
    rerender(<PhaseRibbon view={view} mode="overview" localId="p1" />);
    expect(screen.getByTestId('phase-ribbon').getAttribute('data-mode')).toBe('overview');
    expect(screen.queryByTestId('ribbon-focus')).toBeNull();
  });

  it('degrades gracefully when the turn/active player are unknown (older server)', () => {
    render(<PhaseRibbon view={viewWith(0, '', 'untap')} mode="overview" />);
    expect(screen.getByTestId('ribbon-turn').textContent).toBe('Turn —');
    expect(screen.getByTestId('ribbon-active').textContent).toBe('Active player —');
  });
});
