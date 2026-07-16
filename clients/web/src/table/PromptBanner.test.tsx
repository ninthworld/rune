import { afterEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, render, screen } from '@testing-library/react';
import type { GameView } from '../protocol';
import type { PendingPrompt } from '../store';
import { PromptBanner } from './PromptBanner';

afterEach(cleanup);

/** A minimal live view carrying just the fields the banner reads. */
function view(): GameView {
  return {
    you: 'p1',
    my_hand: [],
    me: { life: 20, library_size: 40 },
    opponents: [],
    battlefield: [],
    stack: [],
    graveyards: [],
    exile: [],
    phase: 'precombat_main',
    turn: 1,
    active_player: 'p1',
    mana_pool: [],
    priority_player: 'p1',
    valid_actions: [],
  };
}

function prompt(deadline?: number): PendingPrompt {
  return { actions: [], globalActions: [], subjectActions: [], deadline };
}

describe('PromptBanner decision countdown (issue #263)', () => {
  it('shows no countdown when the view carries no deadline', () => {
    render(<PromptBanner view={view()} prompt={prompt(undefined)} />);
    expect(screen.queryByTestId('deadline-countdown')).toBeNull();
  });

  it('ticks the deadline down once per second and enters a low-time warning', () => {
    vi.useFakeTimers();
    try {
      render(<PromptBanner view={view()} prompt={prompt(30)} />);
      const countdown = () => screen.getByTestId('deadline-countdown');
      expect(countdown().textContent).toContain('30s');
      expect(countdown().getAttribute('data-low')).toBeNull();

      act(() => vi.advanceTimersByTime(1000));
      expect(countdown().textContent).toContain('29s');

      // Advance to 8s remaining (30 - 22): under the 10s threshold → warning.
      act(() => vi.advanceTimersByTime(21_000));
      expect(countdown().textContent).toContain('8s');
      expect(countdown().getAttribute('data-low')).toBe('true');

      // The countdown clamps at zero rather than going negative.
      act(() => vi.advanceTimersByTime(60_000));
      expect(countdown().textContent).toContain('0s');
    } finally {
      vi.useRealTimers();
    }
  });

  it('re-seeds from a fresh deadline (server re-send / reconnect)', () => {
    vi.useFakeTimers();
    try {
      const { rerender } = render(<PromptBanner view={view()} prompt={prompt(30)} />);
      act(() => vi.advanceTimersByTime(5000));
      expect(screen.getByTestId('deadline-countdown').textContent).toContain('25s');
      // A fresh view carries the true remaining time; the countdown re-seeds to it.
      rerender(<PromptBanner view={view()} prompt={prompt(20)} />);
      expect(screen.getByTestId('deadline-countdown').textContent).toContain('20s');
    } finally {
      vi.useRealTimers();
    }
  });
});
