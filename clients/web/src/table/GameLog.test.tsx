import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import type { GameLogEntry, GameView } from '../protocol';
import { GameLog } from './GameLog';

afterEach(cleanup);

/** A minimal live {@link GameView} carrying just the fields the log panel reads. */
function viewWith(log: GameLogEntry[], names: Record<string, string> = {}): GameView {
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
    valid_actions: [],
    log,
    player_names: names,
  };
}

const NAMES = { p1: 'Alice', p2: 'Bob' };

describe('GameLog (issue #260)', () => {
  it('renders an empty state when the window carries no events', () => {
    render(<GameLog view={viewWith([])} />);
    expect(screen.getByTestId('game-log-empty')).toBeDefined();
    expect(screen.queryByTestId('game-log-list')).toBeNull();
  });

  it('renders each event as client-composed prose, oldest first', () => {
    const log: GameLogEntry[] = [
      {
        sequence: 1,
        event: { type: 'spell_cast', player: 'p2', card: { id: 's1', name: 'Lightning Bolt' } },
      },
      {
        sequence: 2,
        event: {
          type: 'damage_dealt',
          target: { kind: 'permanent', permanent: { id: 'perm_1', name: 'Grizzly Bears' } },
          amount: 3,
        },
      },
    ];
    render(<GameLog view={viewWith(log, NAMES)} />);
    expect(screen.getByTestId('log-entry-1').textContent).toBe('Bob cast Lightning Bolt.');
    expect(screen.getByTestId('log-entry-2').textContent).toBe('Grizzly Bears takes 3 damage.');
    // Order in the DOM is oldest → newest (newest at the bottom).
    const list = screen.getByTestId('game-log-list');
    const ids = within(list)
      .getAllByTestId(/^log-entry-/)
      .map((el) => el.getAttribute('data-testid'));
    expect(ids).toEqual(['log-entry-1', 'log-entry-2']);
  });

  it('collapses a run of consecutive step changes and expands on toggle', () => {
    const log: GameLogEntry[] = [
      {
        sequence: 1,
        event: { type: 'step_changed', turn: 5, active_player: 'p1', phase: 'untap' },
      },
      {
        sequence: 2,
        event: { type: 'step_changed', turn: 5, active_player: 'p1', phase: 'upkeep' },
      },
      { sequence: 3, event: { type: 'step_changed', turn: 5, active_player: 'p1', phase: 'draw' } },
    ];
    render(<GameLog view={viewWith(log, NAMES)} />);
    // Collapsed: only the most recent step shows; the earlier two hide behind the toggle.
    expect(screen.getByTestId('log-steps')).toBeDefined();
    expect(screen.queryByTestId('log-entry-1')).toBeNull();
    expect(screen.queryByTestId('log-entry-2')).toBeNull();
    expect(screen.getByTestId('log-entry-3').textContent).toBe('Turn 5, Draw — Alice');
    const toggle = screen.getByTestId('log-steps-toggle');
    expect(toggle.textContent).toBe('+2 earlier steps');
    // Expanded: every step in the run is now visible.
    fireEvent.click(toggle);
    expect(screen.getByTestId('log-entry-1')).toBeDefined();
    expect(screen.getByTestId('log-entry-2')).toBeDefined();
    expect(screen.getByTestId('log-entry-3')).toBeDefined();
    expect(screen.getByTestId('log-steps-toggle').textContent).toBe('Hide earlier steps');
  });

  it('highlights a permanent reference on click', () => {
    const onHighlight = vi.fn();
    const log: GameLogEntry[] = [
      {
        sequence: 1,
        event: { type: 'permanent_died', permanent: { id: 'perm_1', name: 'Grizzly Bears' } },
      },
    ];
    render(<GameLog view={viewWith(log, NAMES)} onHighlight={onHighlight} />);
    fireEvent.click(screen.getByTestId('log-ref-perm_1'));
    expect(onHighlight).toHaveBeenCalledWith('perm_1');
  });

  it('highlights a player reference on click', () => {
    const onHighlight = vi.fn();
    const log: GameLogEntry[] = [{ sequence: 1, event: { type: 'mulligan', player: 'p2' } }];
    render(<GameLog view={viewWith(log, NAMES)} onHighlight={onHighlight} />);
    const ref = screen.getByTestId('log-ref-p2');
    expect(ref.textContent).toBe('Bob');
    fireEvent.click(ref);
    expect(onHighlight).toHaveBeenCalledWith('p2');
  });

  it('marks the currently-highlighted reference as pressed', () => {
    const log: GameLogEntry[] = [
      {
        sequence: 1,
        event: { type: 'permanent_died', permanent: { id: 'perm_1', name: 'Grizzly Bears' } },
      },
    ];
    render(<GameLog view={viewWith(log, NAMES)} highlightedId="perm_1" />);
    expect(screen.getByTestId('log-ref-perm_1').getAttribute('aria-pressed')).toBe('true');
  });

  it('renders a reference by its record-time name even when the object is gone (dead reference)', () => {
    // A permanent that has left the battlefield is still named from the event; the click
    // simply asks to highlight an id nothing on the board carries — graceful, no crash.
    const onHighlight = vi.fn();
    const log: GameLogEntry[] = [
      {
        sequence: 1,
        event: { type: 'permanent_died', permanent: { id: 'gone_1', name: 'Memnite' } },
      },
    ];
    render(<GameLog view={viewWith(log, NAMES)} onHighlight={onHighlight} />);
    const ref = screen.getByTestId('log-ref-gone_1');
    expect(ref.textContent).toBe('Memnite');
    fireEvent.click(ref);
    expect(onHighlight).toHaveBeenCalledWith('gone_1');
  });

  it('shows the same log on a fresh mount as one that watched the window build up', () => {
    // Reconstructability (acceptance): the panel renders purely from `view.log`, so a
    // client that mounts fresh at a mid-game view sees exactly what a client that saw
    // every incremental frame sees — within the carried window.
    const early: GameLogEntry[] = [
      {
        sequence: 1,
        event: { type: 'spell_cast', player: 'p1', card: { id: 's1', name: 'Bolt' } },
      },
    ];
    const full: GameLogEntry[] = [
      ...early,
      {
        sequence: 2,
        event: { type: 'spell_resolved', player: 'p1', card: { id: 's1', name: 'Bolt' } },
      },
      { sequence: 3, event: { type: 'cards_drawn', player: 'p2', count: 2 } },
    ];

    // A client that watched it build: render early, then rerender at the full window.
    const watched = render(<GameLog view={viewWith(early, NAMES)} />);
    watched.rerender(<GameLog view={viewWith(full, NAMES)} />);
    const watchedText = screen.getByTestId('game-log-list').textContent;
    cleanup();

    // A client that mounts fresh straight at the full window.
    render(<GameLog view={viewWith(full, NAMES)} />);
    const freshText = screen.getByTestId('game-log-list').textContent;

    expect(freshText).toBe(watchedText);
  });

  // ----- Unread activity (issue #340) -----

  it('marks unseen entries distinctly with an AT-visible "New:" prefix', () => {
    const entries: GameLogEntry[] = [
      { sequence: 1, event: { type: 'hand_kept', player: 'p1' } },
      { sequence: 2, event: { type: 'cards_drawn', player: 'p2', count: 1 } },
    ];
    render(
      <GameLog view={viewWith(entries, NAMES)} isUnseen={(seq) => seq === 2} unreadCount={1} />,
    );
    const seen = screen.getByTestId('log-entry-1');
    const unseen = screen.getByTestId('log-entry-2');
    expect(seen.getAttribute('data-unseen')).toBeNull();
    expect(unseen.getAttribute('data-unseen')).toBe('true');
    // The distinction has a text form for assistive technology, not hue alone.
    expect(unseen.textContent).toContain('New:');
  });

  it('offers a jump-to-newest affordance that reports the log seen', () => {
    const entries: GameLogEntry[] = [
      { sequence: 1, event: { type: 'hand_kept', player: 'p1' } },
      { sequence: 2, event: { type: 'cards_drawn', player: 'p2', count: 1 } },
    ];
    const onSeen = vi.fn();
    render(
      <GameLog
        view={viewWith(entries, NAMES)}
        isUnseen={(seq) => seq === 2}
        unreadCount={1}
        onSeen={onSeen}
      />,
    );
    const jump = screen.getByTestId('log-unread');
    expect(jump.getAttribute('aria-label')).toContain('1 new');
    fireEvent.click(jump);
    expect(onSeen).toHaveBeenCalledTimes(1);
  });

  it('shows no unread affordance when nothing is unread', () => {
    const entries: GameLogEntry[] = [{ sequence: 1, event: { type: 'hand_kept', player: 'p1' } }];
    render(<GameLog view={viewWith(entries, NAMES)} unreadCount={0} />);
    expect(screen.queryByTestId('log-unread')).toBeNull();
  });
});
