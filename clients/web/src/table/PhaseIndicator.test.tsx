import { afterEach, describe, expect, it } from 'vitest';
import { cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import type { GameView, Phase } from '../protocol';
import { PHASES } from '../protocol';
import { PhaseIndicator } from './PhaseIndicator';

afterEach(cleanup);

/** A minimal live view carrying just the fields the indicator reads. */
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
    seat_order: [],
    mana_pool: [],
    valid_actions: [],
    player_names: {},
  };
}

describe('PhaseIndicator (issue #297)', () => {
  it('renders compactly by default: turn, active player, current step, no full strip', () => {
    render(
      <PhaseIndicator view={viewWith(4, 'p2', 'declare_attackers')} mode="overview" localId="p1" />,
    );
    expect(screen.getByTestId('indicator-turn').textContent).toBe('Turn 4');
    // p2's turn from p1's seat reads as the opponent's turn, not "Your turn".
    expect(screen.getByTestId('indicator-active').textContent).toBe("p2's turn");
    // The current step name shows compactly.
    expect(screen.getByTestId('indicator-step').textContent).toBe('Declare Attackers');
    // The twelve-step strip is NOT present until expanded (retired always-on ribbon).
    expect(screen.queryByTestId('indicator-steps')).toBeNull();
  });

  it('marks the current phase group in the compact progress treatment', () => {
    render(
      <PhaseIndicator view={viewWith(4, 'p2', 'declare_attackers')} mode="overview" localId="p1" />,
    );
    // declare_attackers belongs to the combat group; exactly it is marked current.
    expect(screen.getByTestId('indicator-group-combat').getAttribute('data-current')).toBe('true');
    expect(screen.getByTestId('indicator-group-beginning').getAttribute('data-current')).toBeNull();
    // Groups before the current one read as passed; later ones do not.
    expect(screen.getByTestId('indicator-group-beginning').getAttribute('data-passed')).toBe(
      'true',
    );
    expect(screen.getByTestId('indicator-group-ending').getAttribute('data-passed')).toBeNull();
  });

  it('expands on demand to the full twelve-step sequence with the current one marked', () => {
    render(
      <PhaseIndicator view={viewWith(4, 'p2', 'declare_attackers')} mode="overview" localId="p1" />,
    );
    const toggle = screen.getByTestId('indicator-toggle');
    expect(toggle.getAttribute('aria-expanded')).toBe('false');

    fireEvent.click(toggle);
    expect(toggle.getAttribute('aria-expanded')).toBe('true');

    const list = screen.getByTestId('indicator-steps');
    // All twelve steps appear, each individually addressable (stable per-step handle
    // for the future stop toggles, #264).
    expect(within(list).getAllByRole('listitem')).toHaveLength(PHASES.length);
    for (const phase of PHASES) {
      expect(screen.getByTestId(`indicator-step-${phase}`).getAttribute('data-phase')).toBe(phase);
    }
    // Exactly the current step carries aria-current="step".
    const current = screen.getByTestId('indicator-step-declare_attackers');
    expect(current.getAttribute('aria-current')).toBe('step');
    expect(screen.getByTestId('indicator-step-upkeep').getAttribute('aria-current')).toBeNull();

    // Collapsing hides the strip again.
    fireEvent.click(toggle);
    expect(toggle.getAttribute('aria-expanded')).toBe('false');
    expect(screen.queryByTestId('indicator-steps')).toBeNull();
  });

  it('expand/collapse is reachable by keyboard (the toggle is a button)', () => {
    render(<PhaseIndicator view={viewWith(1, 'p1', 'upkeep')} mode="overview" localId="p1" />);
    const toggle = screen.getByRole('button', { name: /turn/i });
    toggle.focus();
    expect(document.activeElement).toBe(toggle);
    // Enter/Space activate a native button; fireEvent.click models that activation.
    fireEvent.click(toggle);
    expect(screen.getByTestId('indicator-steps')).toBeDefined();
  });

  it('is a role="status" region announcing turn/step', () => {
    render(<PhaseIndicator view={viewWith(3, 'p1', 'upkeep')} mode="overview" localId="p1" />);
    const status = screen.getByRole('status');
    expect(status.getAttribute('data-testid')).toBe('phase-indicator');
  });

  it('renders collapsed on a fresh mount (expansion is ephemeral, not load-bearing)', () => {
    // A brand-new mount from one GameView is always collapsed — no client state
    // carries across messages.
    render(<PhaseIndicator view={viewWith(2, 'p1', 'draw')} mode="overview" localId="p1" />);
    expect(screen.getByTestId('indicator-toggle').getAttribute('aria-expanded')).toBe('false');
    expect(screen.queryByTestId('indicator-steps')).toBeNull();
  });

  it('phrases the receiver’s own turn as "Your turn"', () => {
    render(<PhaseIndicator view={viewWith(1, 'p1', 'upkeep')} mode="overview" localId="p1" />);
    expect(screen.getByTestId('indicator-active').textContent).toBe('Your turn');
  });

  it('labels the active opponent by display name when the server sent one (issue #294)', () => {
    const view = { ...viewWith(4, 'p2', 'draw'), player_names: { p2: 'Bob' } };
    render(<PhaseIndicator view={view} mode="overview" localId="p1" />);
    expect(screen.getByTestId('indicator-active').textContent).toBe("Bob's turn");
  });

  it('shows a decision badge in focus mode and none in overview', () => {
    const view = viewWith(2, 'p1', 'precombat_main');
    const { rerender } = render(<PhaseIndicator view={view} mode="focus" localId="p1" />);
    expect(screen.getByTestId('phase-indicator').getAttribute('data-mode')).toBe('focus');
    expect(screen.getByTestId('indicator-decision')).toBeDefined();
    rerender(<PhaseIndicator view={view} mode="overview" localId="p1" />);
    expect(screen.getByTestId('phase-indicator').getAttribute('data-mode')).toBe('overview');
    expect(screen.queryByTestId('indicator-decision')).toBeNull();
  });

  it('degrades gracefully when the turn/active player are unknown (older server)', () => {
    render(<PhaseIndicator view={viewWith(0, '', 'untap')} mode="overview" />);
    expect(screen.getByTestId('indicator-turn').textContent).toBe('Turn —');
    expect(screen.getByTestId('indicator-active').textContent).toBe('Active player —');
  });
});

describe('PhaseIndicator priority stops and auto-pass (issue #264)', () => {
  it('shows the auto-pass indicator only when the view flags it', () => {
    const base = viewWith(2, 'p1', 'precombat_main');
    const { rerender } = render(<PhaseIndicator view={base} mode="overview" localId="p1" />);
    expect(screen.queryByTestId('auto-passed-indicator')).toBeNull();

    rerender(<PhaseIndicator view={{ ...base, auto_passed: true }} mode="overview" localId="p1" />);
    expect(screen.getByTestId('auto-passed-indicator')).toBeDefined();
  });

  it('renders a per-step stop toggle in the expanded list, reflecting the current set', () => {
    const view = { ...viewWith(2, 'p1', 'upkeep'), stops: ['end'] as Phase[] };
    render(<PhaseIndicator view={view} mode="overview" localId="p1" onSetStops={() => {}} />);
    fireEvent.click(screen.getByTestId('indicator-toggle'));

    // Every step has a toggle; the one in the current set reads pressed, others not.
    for (const phase of PHASES) {
      expect(screen.getByTestId(`stop-toggle-${phase}`)).toBeDefined();
    }
    expect(screen.getByTestId('stop-toggle-end').getAttribute('aria-pressed')).toBe('true');
    expect(screen.getByTestId('stop-toggle-upkeep').getAttribute('aria-pressed')).toBe('false');
  });

  it('toggling an unset step sends the full new set including it', () => {
    let sent: Phase[] | null = null;
    const view = { ...viewWith(2, 'p1', 'upkeep'), stops: ['end'] as Phase[] };
    render(
      <PhaseIndicator
        view={view}
        mode="overview"
        localId="p1"
        onSetStops={(s) => {
          sent = s;
        }}
      />,
    );
    fireEvent.click(screen.getByTestId('indicator-toggle'));
    fireEvent.click(screen.getByTestId('stop-toggle-upkeep'));
    expect(sent).toEqual(['end', 'upkeep']);
  });

  it('toggling a set step removes it from the set', () => {
    let sent: Phase[] | null = null;
    const view = { ...viewWith(2, 'p1', 'upkeep'), stops: ['end', 'upkeep'] as Phase[] };
    render(
      <PhaseIndicator
        view={view}
        mode="overview"
        localId="p1"
        onSetStops={(s) => {
          sent = s;
        }}
      />,
    );
    fireEvent.click(screen.getByTestId('indicator-toggle'));
    fireEvent.click(screen.getByTestId('stop-toggle-end'));
    expect(sent).toEqual(['upkeep']);
  });

  it('renders no stop toggles when no setter is wired (read-only game-over board)', () => {
    render(<PhaseIndicator view={viewWith(2, 'p1', 'upkeep')} mode="overview" localId="p1" />);
    fireEvent.click(screen.getByTestId('indicator-toggle'));
    expect(screen.queryByTestId('stop-toggle-upkeep')).toBeNull();
  });
});
