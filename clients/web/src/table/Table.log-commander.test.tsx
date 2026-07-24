import { describe, expect, it } from 'vitest';
import { fireEvent, render, screen, within } from '@testing-library/react';
import { COMMANDER_GAME_VIEW_JSON, SAMPLE_GAME_VIEW_JSON } from '../game-view.fixture';
import { Table } from './Table';
import { registerTableTestHooks, seed } from './table-test-support';

registerTableTestHooks();

describe('Table game log (issue #260)', () => {
  it('renders the game log in the rail with client-composed prose', () => {
    seed(SAMPLE_GAME_VIEW_JSON);
    render(<Table />);
    const log = screen.getByTestId('game-log');
    // A spell-cast entry composed client-side from the structured event.
    expect(within(log).getByTestId('log-entry-35').textContent).toBe('p2 cast Lightning Bolt.');
    // The leading run of consecutive step changes collapses behind one summary.
    expect(within(log).getByTestId('log-steps')).toBeDefined();
  });

  it('highlights a referenced player tile on click, and toggles it off', () => {
    seed(SAMPLE_GAME_VIEW_JSON);
    render(<Table />);
    // The cast entry references p2; before any click the tile is not highlighted.
    expect(screen.getByTestId('tile-p2').getAttribute('data-highlighted')).toBeNull();
    fireEvent.click(within(screen.getByTestId('game-log')).getByTestId('log-ref-p2'));
    expect(screen.getByTestId('tile-p2').getAttribute('data-highlighted')).toBe('true');
    // Clicking the same reference again clears the highlight (ephemeral, presentational).
    fireEvent.click(within(screen.getByTestId('game-log')).getByTestId('log-ref-p2'));
    expect(screen.getByTestId('tile-p2').getAttribute('data-highlighted')).toBeNull();
  });

  it('highlighting a log reference opens no action tray (purely presentational)', () => {
    const choose = seed(SAMPLE_GAME_VIEW_JSON);
    render(<Table />);
    fireEvent.click(within(screen.getByTestId('game-log')).getByTestId('log-ref-perm_xyz'));
    // No selection echo / entity actions surface — highlighting derives nothing.
    expect(screen.queryByTestId('entity-actions-perm_xyz')).toBeNull();
    expect(choose).not.toHaveBeenCalled();
  });

  it('keeps the log visible and interactive in the read-only game-over state', () => {
    // A terminal frame that still carries a history window: the log rides the rail in
    // the game-over branch too, and its references still highlight (issue #260 scope).
    const gameOverWithLog = JSON.stringify({
      you: 'p1',
      opponents: [{ player_id: 'p2', hand_size: 0, life: 0, library_size: 40 }],
      phase: 'end',
      valid_actions: [],
      result: { winner: 'p1', losers: ['p2'], reason: 'life_zero' },
      log: [
        {
          sequence: 1,
          event: { type: 'spell_cast', player: 'p2', card: { id: 's9', name: 'Shock' } },
        },
        {
          sequence: 2,
          event: {
            type: 'game_over',
            result: { winner: 'p1', losers: ['p2'], reason: 'life_zero' },
          },
        },
      ],
    });
    seed(gameOverWithLog);
    render(<Table />);
    expect(screen.getByTestId('table-game-over')).toBeDefined();
    const log = screen.getByTestId('game-log');
    expect(within(log).getByTestId('log-entry-2').textContent).toBe(
      'Game over — p1 wins (life total reached zero).',
    );
    // A reference still highlights the player's tile in the read-only terminal state.
    fireEvent.click(within(log).getByTestId('log-ref-p2'));
    expect(screen.getByTestId('tile-p2').getAttribute('data-highlighted')).toBe('true');
  });
});

describe('Table commander chrome (issue #372)', () => {
  it('renders the command-zone piles, the recast tax, and the commander-damage tally', () => {
    seed(COMMANDER_GAME_VIEW_JSON);
    render(<Table />);

    // The receiver's own command zone rides with their bottom-shell piles.
    const mePiles = screen.getByTestId('me-piles');
    expect(within(mePiles).getByTestId('command-pile-p1')).toBeDefined();

    // The opponent's command zone shows on their panel, with the {2} recast tax beside it.
    expect(screen.getByTestId('command-pile-p2')).toBeDefined();
    expect(screen.getByTestId('cmd-tax-p2').textContent).toContain('Tax {2}');

    // The commander damage the receiver has taken reads as `amount/21`.
    expect(screen.getByTestId('cmd-damage-p1').textContent).toContain('7/21');
  });

  it('shows no commander chrome in a plain (non-commander) frame', () => {
    seed(SAMPLE_GAME_VIEW_JSON);
    render(<Table />);
    expect(screen.queryByTestId('command-pile-p1')).toBeNull();
    expect(screen.queryByTestId('command-pile-p2')).toBeNull();
    expect(screen.queryByTestId('cmd-tax-p2')).toBeNull();
    expect(screen.queryByTestId('cmd-damage-p1')).toBeNull();
  });
});
