/**
 * Session moments on the production match surface (issue #509,
 * `docs/design/visual-system.md` §8): the game-start assembly, the mulligan card
 * moment, and the postgame exit that recedes into the lobby.
 *
 * Every assertion here is about the three binding contracts — the moment stays
 * inside its budget, it is skippable where §8 marks it, it snaps under reduced
 * motion, and it **never gates input**. The reconnect row keeps its own suite in
 * `LiveMatchTable.reconnect.test.tsx`.
 */
import { act, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  GAME_OVER_DRAW_JSON,
  GAME_OVER_LOSS_JSON,
  GAME_OVER_WIN_JSON,
  MULLIGAN_GAME_VIEW_JSON,
  SAMPLE_GAME_VIEW_JSON,
} from '../../game-view.fixture';
import { useGameStore } from '../../store';
import { registerTableTestHooks, seed } from '../table-test-support';
import { LiveMatchTable } from './LiveMatchTable';
import { momentBudgetMs } from './sessionMoments';

vi.mock('../EffectsSurface', () => ({
  EffectsSurface: () => <div data-testid="effects-surface" aria-hidden="true" />,
}));
vi.mock('../effects', () => ({
  EffectsLayer: class {
    setPersistent(): void {}
    replaceTransients(): void {}
    trackMotion(): void {}
  },
}));

registerTableTestHooks();

/** Force the OS reduced-motion query on or off for one render. */
function setOsReducedMotion(reduce: boolean): void {
  vi.stubGlobal('matchMedia', (query: string) => ({
    matches: reduce && query.includes('prefers-reduced-motion'),
    media: query,
    addEventListener: () => {},
    removeEventListener: () => {},
  }));
}

describe('LiveMatchTable session moments', () => {
  beforeEach(() => {
    vi.spyOn(window, 'requestAnimationFrame').mockReturnValue(1);
    vi.spyOn(window, 'cancelAnimationFrame').mockImplementation(() => undefined);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    vi.useRealTimers();
  });

  it('stages the game start inside its ≤ 800 ms window and settles itself', () => {
    vi.useFakeTimers();
    seed(SAMPLE_GAME_VIEW_JSON);
    render(<LiveMatchTable />);
    const shell = screen.getByTestId('live-match-table');

    expect(shell.dataset.moment).toBe('game-start');
    // Input is live from the first frame — the assembly never gates it.
    expect(screen.getByTestId<HTMLButtonElement>('live-hand-card-c1').disabled).toBe(false);

    act(() => {
      vi.advanceTimersByTime(momentBudgetMs('game-start'));
    });
    expect(shell.dataset.moment).toBeUndefined();
  });

  it('skips the game start on deliberate input (§8 marks it skippable)', () => {
    vi.useFakeTimers();
    seed(SAMPLE_GAME_VIEW_JSON);
    render(<LiveMatchTable />);
    const shell = screen.getByTestId('live-match-table');
    expect(shell.dataset.moment).toBe('game-start');

    act(() => {
      fireEvent.pointerDown(window);
    });
    expect(shell.dataset.moment).toBeUndefined();
  });

  it('presents a view that lands mid-moment immediately, never queued behind it', () => {
    vi.useFakeTimers();
    seed(SAMPLE_GAME_VIEW_JSON);
    render(<LiveMatchTable />);
    const shell = screen.getByTestId('live-match-table');
    expect(shell.dataset.moment).toBe('game-start');

    // A newer authoritative view arriving while the assembly ramps is rendered
    // at once and is fully answerable: gameplay is never queued behind a
    // session moment, and the moment still retires on its own budget.
    act(() => useGameStore.getState().ingest(MULLIGAN_GAME_VIEW_JSON));
    expect(screen.getByTestId('multiselect-options')).toBeTruthy();
    expect(screen.getByTestId<HTMLButtonElement>('live-hand-card-card_a').disabled).toBe(false);
    act(() => {
      vi.advanceTimersByTime(momentBudgetMs('game-start'));
    });
    expect(shell.dataset.moment).toBeUndefined();
  });

  it('replaces the assembly with the reconnect cue when a rebuild lands mid-moment', () => {
    vi.useFakeTimers();
    act(() => {
      useGameStore.setState({ sessionEpoch: 1 });
    });
    seed(SAMPLE_GAME_VIEW_JSON);
    render(<LiveMatchTable />);
    const shell = screen.getByTestId('live-match-table');
    expect(shell.dataset.moment).toBe('game-start');

    act(() => {
      useGameStore.setState((state) => ({ sessionEpoch: state.sessionEpoch + 1 }));
      useGameStore.getState().ingest(SAMPLE_GAME_VIEW_JSON);
    });

    expect(shell.dataset.moment).toBe('reconnect');
    expect(shell.dataset.orienting).toBe('true');
    act(() => {
      vi.advanceTimersByTime(momentBudgetMs('reconnect'));
    });
    expect(shell.dataset.moment).toBeUndefined();
  });

  it('never stages a moment under reduced motion', () => {
    setOsReducedMotion(true);
    seed(SAMPLE_GAME_VIEW_JSON);
    render(<LiveMatchTable />);
    const shell = screen.getByTestId('live-match-table');

    // Not "staged then snapped" — no staged frame is ever produced.
    expect(shell.dataset.moment).toBeUndefined();
    expect(shell.dataset.orienting).toBeUndefined();
  });

  it('presents the hand as a card moment while the mulligan decision is forced', () => {
    seed(MULLIGAN_GAME_VIEW_JSON);
    render(<LiveMatchTable />);
    const shell = screen.getByTestId('live-match-table');

    // The mulligan is the one decision the view leaves no way around (#451); the
    // hand is lifted forward as the subject of the question, and stays fully
    // interactive underneath — the sheet passes pointer events through.
    expect(shell.dataset.forcedDecision).toBe('true');
    expect(screen.getByTestId('decision-sheet').dataset.pointerThrough).toBe('true');
    fireEvent.click(screen.getByTestId('live-hand-card-card_a'));
    expect(screen.getByTestId('live-hand-card-card_a').getAttribute('aria-pressed')).toBe('true');

    // Ordinary play carries no card moment.
    act(() => useGameStore.getState().ingest(SAMPLE_GAME_VIEW_JSON));
    expect(shell.dataset.forcedDecision).toBeUndefined();
  });
});

describe('LiveMatchTable postgame exit (issues #452 + #509)', () => {
  beforeEach(() => {
    vi.spyOn(window, 'requestAnimationFrame').mockReturnValue(1);
    vi.spyOn(window, 'cancelAnimationFrame').mockImplementation(() => undefined);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    vi.useRealTimers();
  });

  const terminal: Array<[string, string]> = [
    ['a win', GAME_OVER_WIN_JSON],
    ['a loss', GAME_OVER_LOSS_JSON],
    ['a draw', GAME_OVER_DRAW_JSON],
  ];

  it.each(terminal)('recedes into the lobby from %s', (_label, json) => {
    vi.useFakeTimers();
    const leaveGame = vi.fn();
    seed(json);
    useGameStore.setState({ leaveGame });
    render(<LiveMatchTable />);
    const shell = screen.getByTestId('live-match-table');

    fireEvent.click(screen.getByTestId('game-over-leave'));

    // The scene recedes first, then the store transition runs — one exit, once.
    expect(shell.dataset.moment).toBe('return-to-lobby');
    expect(leaveGame).not.toHaveBeenCalled();
    act(() => {
      vi.advanceTimersByTime(momentBudgetMs('return-to-lobby'));
    });
    expect(leaveGame).toHaveBeenCalledTimes(1);
  });

  it('ignores a second exit while the recede is in flight', () => {
    vi.useFakeTimers();
    const leaveGame = vi.fn();
    seed(GAME_OVER_WIN_JSON);
    useGameStore.setState({ leaveGame });
    render(<LiveMatchTable />);

    const exit = screen.getByTestId('game-over-leave');
    fireEvent.click(exit);
    fireEvent.click(exit);
    act(() => {
      vi.advanceTimersByTime(momentBudgetMs('return-to-lobby') * 3);
    });

    expect(leaveGame).toHaveBeenCalledTimes(1);
  });

  it('cuts straight to the lobby under reduced motion (§8 RM form)', () => {
    setOsReducedMotion(true);
    const leaveGame = vi.fn();
    seed(GAME_OVER_LOSS_JSON);
    useGameStore.setState({ leaveGame });
    render(<LiveMatchTable />);

    fireEvent.click(screen.getByTestId('game-over-leave'));

    expect(leaveGame).toHaveBeenCalledTimes(1);
    expect(screen.getByTestId('live-match-table').dataset.moment).toBeUndefined();
  });
});
