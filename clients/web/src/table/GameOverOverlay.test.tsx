import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import type { GameOverReason, GameResult } from '../protocol';
import { GameOverOverlay } from './GameOverOverlay';

afterEach(cleanup);

/** Render the overlay for a result from the `you` seat's perspective. */
function renderOverlay(you: string, result: GameResult, names: Record<string, string> = {}): void {
  render(<GameOverOverlay result={result} you={you} names={names} />);
}

describe('GameOverOverlay verdict from the receiver’s seat (issue #141)', () => {
  it('announces Victory when the receiver is the winner', () => {
    renderOverlay('p1', { winner: 'p1', losers: ['p2'], reason: 'concede' });
    expect(screen.getByTestId('game-over-headline').textContent).toBe('Victory');
    expect(screen.getByTestId('game-over-winner').textContent).toContain('p1 wins');
  });

  it('announces Defeat when another player is the winner', () => {
    renderOverlay('p1', { winner: 'p2', losers: ['p1'], reason: 'life_zero' });
    expect(screen.getByTestId('game-over-headline').textContent).toBe('Defeat');
    expect(screen.getByTestId('game-over-winner').textContent).toContain('p2 wins');
  });

  it('announces Draw when there is no winner (CR 104.4a)', () => {
    renderOverlay('p1', { losers: ['p1', 'p2'], reason: 'life_zero' });
    expect(screen.getByTestId('game-over-headline').textContent).toBe('Draw');
    expect(screen.getByTestId('game-over-winner').textContent).toContain('draw');
  });

  it('exposes the result as an alertdialog for assistive tech', () => {
    renderOverlay('p1', { winner: 'p1', losers: ['p2'], reason: 'decked' });
    expect(screen.getByRole('alertdialog', { name: 'Game over' })).toBeDefined();
  });

  it('names the winner by display name when the server sent one (issue #294)', () => {
    renderOverlay('p1', { winner: 'p2', losers: ['p1'], reason: 'life_zero' }, { p2: 'Bob' });
    expect(screen.getByTestId('game-over-winner').textContent).toContain('Bob wins');
  });
});

describe('GameOverOverlay reason text (each losing condition)', () => {
  const cases: Array<[GameOverReason, RegExp]> = [
    ['life_zero', /life total reached zero/i],
    ['decked', /empty library/i],
    ['concede', /conceded/i],
  ];

  it.each(cases)('phrases the %s reason', (reason, matcher) => {
    renderOverlay('p1', { winner: 'p1', losers: ['p2'], reason });
    expect(screen.getByTestId('game-over-reason').textContent).toMatch(matcher);
  });

  it('offers a way out of every terminal state (issue #452)', () => {
    // Regression: the verdict rendered as text alone, with no control anywhere on
    // the in-game path to leave — win, loss, draw, and concede alike dead-ended.
    const cases: GameResult[] = [
      { winner: 'p1', losers: ['p2'], reason: 'life_zero' },
      { winner: 'p2', losers: ['p1'], reason: 'concede' },
      { losers: ['p1', 'p2'], reason: 'life_zero' },
    ];
    for (const result of cases) {
      const onLeave = vi.fn();
      render(<GameOverOverlay result={result} you="p1" names={{}} onLeave={onLeave} />);
      fireEvent.click(screen.getByTestId('game-over-leave'));
      expect(onLeave).toHaveBeenCalledTimes(1);
      cleanup();
    }
  });

  it('falls back generically for an unrecognized future reason', () => {
    // Forward compat: the wire type is closed, but an unknown value must not crash
    // the overlay — it still shows game over with a generic reason line.
    renderOverlay('p1', {
      winner: 'p1',
      losers: ['p2'],
      reason: 'some_future_reason' as GameOverReason,
    });
    expect(screen.getByTestId('game-over-reason').textContent).toMatch(/game has ended/i);
  });
});
