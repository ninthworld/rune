import { afterEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import type { GameOverReason, GameResult } from '../protocol';
import { GameOverOverlay } from './GameOverOverlay';
import { momentBudgetMs } from './live/sessionMoments';

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

  it('reaches the exit from every terminal state, including a reconnect into one', () => {
    // Issue #509 acceptance: win, loss, draw, concede, and a spectator watching a
    // finished game all land on the same working exit — no reachable dead screen.
    const cases: Array<[string, GameResult]> = [
      ['p1', { winner: 'p1', losers: ['p2'], reason: 'life_zero' }],
      ['p1', { winner: 'p2', losers: ['p1'], reason: 'concede' }],
      ['p1', { losers: ['p1', 'p2'], reason: 'life_zero' }],
      ['', { winner: 'p2', losers: ['p1'], reason: 'decked' }],
    ];
    for (const [you, result] of cases) {
      const onLeave = vi.fn();
      render(<GameOverOverlay result={result} you={you} names={{}} onLeave={onLeave} />);
      const exit = screen.getByTestId<HTMLButtonElement>('game-over-leave');
      // Reachable *while the moment stages* — never gated behind it.
      expect(screen.getByTestId('game-over-overlay').dataset.staging).toBe('true');
      expect(exit.disabled).toBe(false);
      fireEvent.click(exit);
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

describe('GameOverOverlay as a session moment (issue #509)', () => {
  const win: GameResult = { winner: 'p1', losers: ['p2'], reason: 'life_zero' };
  const loss: GameResult = { winner: 'p2', losers: ['p1'], reason: 'concede' };
  const drawn: GameResult = { losers: ['p1', 'p2'], reason: 'life_zero' };

  it('stages the verdict in its §8 window and settles itself', () => {
    vi.useFakeTimers();
    try {
      render(<GameOverOverlay result={win} you="p1" names={{}} />);
      const overlay = screen.getByTestId('game-over-overlay');
      expect(overlay.dataset.moment).toBe('victory');
      expect(overlay.dataset.staging).toBe('true');
      // The victory window is ≤ 800 ms and the moment retires itself.
      expect(overlay.style.getPropertyValue('--rune-verdict-ms')).toBe('800ms');
      act(() => {
        vi.advanceTimersByTime(momentBudgetMs('victory'));
      });
      expect(overlay.dataset.staging).toBeUndefined();
    } finally {
      vi.useRealTimers();
    }
  });

  it('skips the victory bloom on deliberate input (§8 marks it skippable)', () => {
    vi.useFakeTimers();
    try {
      render(<GameOverOverlay result={win} you="p1" names={{}} />);
      const overlay = screen.getByTestId('game-over-overlay');
      expect(overlay.dataset.staging).toBe('true');
      act(() => {
        fireEvent.keyDown(window, { key: 'Enter' });
      });
      expect(overlay.dataset.staging).toBeUndefined();
    } finally {
      vi.useRealTimers();
    }
  });

  it('wears the loss family for a defeat or concede and stays neutral on a draw', () => {
    render(<GameOverOverlay result={loss} you="p1" names={{}} />);
    expect(screen.getByTestId('game-over-overlay').dataset.moment).toBe('defeat');
    cleanup();

    render(<GameOverOverlay result={drawn} you="p1" names={{}} />);
    expect(screen.getByTestId('game-over-overlay').dataset.moment).toBe('draw');
    cleanup();

    // A spectator is told who won without wearing anyone's verdict (#504 owns
    // anything beyond the shared panel).
    render(<GameOverOverlay result={win} you="" names={{}} />);
    expect(screen.getByTestId('game-over-overlay').dataset.moment).toBe('draw');
  });

  it('snaps to the settled verdict under reduced motion', () => {
    render(<GameOverOverlay result={win} you="p1" names={{}} reducedMotion />);
    const overlay = screen.getByTestId('game-over-overlay');
    // No staged frame at all: the panel is at its end state from the first render.
    expect(overlay.dataset.staging).toBeUndefined();
    expect(overlay.style.getPropertyValue('--rune-verdict-ms')).toBe('0ms');
    expect(screen.getByTestId('game-over-headline').textContent).toBe('Victory');
  });
});
