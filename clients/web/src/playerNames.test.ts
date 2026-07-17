import { describe, expect, it } from 'vitest';
import { playerName, seatDisplayName } from './playerNames';
import type { GameView, SeatView } from './protocol';

/** A minimal view carrying only the names map the helper reads. */
function viewWithNames(names: Record<string, string>): Pick<GameView, 'player_names'> {
  return { player_names: names };
}

describe('playerName (issue #294)', () => {
  it('returns the chosen name when the server sent one', () => {
    expect(playerName(viewWithNames({ p1: 'Alice' }), 'p1')).toBe('Alice');
  });

  it('falls back to the raw id when no name is present (older server / unnamed)', () => {
    expect(playerName(viewWithNames({}), 'p2')).toBe('p2');
    // An empty-string name is treated as unset, not shown.
    expect(playerName(viewWithNames({ p2: '' }), 'p2')).toBe('p2');
  });
});

describe('seatDisplayName (issue #294)', () => {
  it('uses the occupant’s chosen name when set', () => {
    const seat: SeatView = { seat: 0, occupied_by: 'p1', name: 'Alice' };
    expect(seatDisplayName(seat)).toBe('Alice');
  });

  it('falls back to a 1-based seat label from the real seat index (never the id)', () => {
    const seat: SeatView = { seat: 1, occupied_by: 'p2' };
    expect(seatDisplayName(seat)).toBe('Player 2');
  });
});
