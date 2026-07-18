import { afterEach, describe, expect, it } from 'vitest';
import { cleanup, render, screen, within } from '@testing-library/react';
import { normalizeSpectatorView } from '../wire';
import type { SpectatorView } from '../protocol';
import { SpectatorTable } from './SpectatorTable';

/** A live 3-seat spectator view: one eliminated seat, a permanent on the board, and
 * public graveyard piles — the read fixture the spectate-mode tests build on. */
function spectatorView(overrides: Partial<Record<string, unknown>> = {}): SpectatorView {
  return normalizeSpectatorView({
    players: [
      { player_id: 'p0', hand_size: 4, life: 18, library_size: 33, graveyard_size: 1 },
      {
        player_id: 'p1',
        hand_size: 0,
        life: 0,
        library_size: 0,
        graveyard_size: 3,
        eliminated: true,
      },
      { player_id: 'p2', hand_size: 6, life: 20, library_size: 34, graveyard_size: 0 },
    ],
    battlefield: [
      {
        id: 'perm_1',
        controller: 'p0',
        owner: 'p0',
        card: {
          id: 'perm_1',
          name: 'Grizzly Bears',
          type_line: 'Creature — Bear',
          power: '2',
          toughness: '2',
        },
      },
    ],
    graveyards: [{ player_id: 'p0', cards: [{ id: 'gy_0', name: 'Shock', type_line: 'Instant' }] }],
    phase: 'precombat_main',
    turn: 9,
    active_player: 'p0',
    seat_order: ['p0', 'p1', 'p2'],
    priority_player: 'p0',
    ...overrides,
  });
}

afterEach(cleanup);

describe('SpectatorTable (ADR 0022, issue #351)', () => {
  it('renders a read-only live board with no hand row or action tray', () => {
    render(<SpectatorTable view={spectatorView()} />);
    // The spectate shell and its badge are present…
    expect(screen.getByTestId('spectator-table')).toBeDefined();
    expect(screen.getByTestId('spectator-badge').textContent).toContain('Spectating');
    // …but there is no action tray and no local hand/dock — nothing to play.
    expect(screen.queryByTestId('action-bar')).toBeNull();
    expect(screen.queryByTestId('local-dock')).toBeNull();
    expect(screen.queryByTestId('hud-mana')).toBeNull();
  });

  it('shows every seat’s public state (each player is an opponent tile)', () => {
    render(<SpectatorTable view={spectatorView()} />);
    const hud = screen.getByTestId('opponent-hud');
    // All three seats appear in the HUD — no privileged "self".
    for (const id of ['p0', 'p1', 'p2']) {
      expect(within(hud).getByTestId(`tile-${id}`)).toBeDefined();
    }
    expect(within(hud).getByTestId('hud-life-p0').textContent).toBe('18');
  });

  it('reconstructs the public board from one view (mid-game join)', () => {
    // A fresh mount from a single SpectatorView renders the board — no history needed.
    render(<SpectatorTable view={spectatorView()} />);
    // The public permanent is inspectable (a read-only surface exists for it).
    expect(screen.getByTestId('inspect-surface-perm_1')).toBeDefined();
    // The opponent's public graveyard pile is browsable on the board.
    expect(screen.getByTestId('table-graveyard-p0')).toBeDefined();
  });

  it('shows the terminal verdict when the game is over', () => {
    render(
      <SpectatorTable
        view={spectatorView({
          result: { winner: 'p2', losers: ['p0', 'p1'], reason: 'life_zero' },
        })}
      />,
    );
    expect(screen.getByTestId('game-over-overlay')).toBeDefined();
  });
});
