import { afterEach, describe, expect, it } from 'vitest';
import { cleanup, fireEvent, render, screen, within } from '@testing-library/react';
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
  it('renders the read-only fixed shell: top bar, badge, and no action surfaces', () => {
    render(<SpectatorTable view={spectatorView()} />);
    // The spectate shell rides the fixed anatomy: top bar up top, the badge where
    // the receiver's identity panel would live…
    expect(screen.getByTestId('spectator-table')).toBeDefined();
    expect(screen.getByTestId('top-bar')).toBeDefined();
    expect(screen.getByTestId('spectator-badge').textContent).toContain('Spectating');
    // …but there is no action tray and no local hand/dock — nothing to play…
    expect(screen.queryByTestId('action-bar')).toBeNull();
    expect(screen.queryByTestId('local-dock')).toBeNull();
    expect(screen.queryByTestId('hud-mana')).toBeNull();
    // …and nothing on the board is selectable or targetable: the public permanent
    // carries only its transparent inspect surface.
    expect(screen.queryByTestId('entity-perm_1')).toBeNull();
    expect(screen.queryByTestId('target-perm_1')).toBeNull();
    expect(screen.getByTestId('inspect-surface-perm_1')).toBeDefined();
  });

  it('gives every seat its own bounded player panel (no privileged "self")', () => {
    render(<SpectatorTable view={spectatorView()} />);
    const chrome = screen.getByTestId('panel-chrome');
    // With no receiver, the scene folds the you-frame into the pool: all three
    // seats get a panel, a header tile, and their public zone piles.
    for (const id of ['p0', 'p1', 'p2']) {
      expect(within(chrome).getByTestId(`player-panel-${id}`)).toBeDefined();
      expect(within(chrome).getByTestId(`tile-${id}`)).toBeDefined();
      expect(within(chrome).getByTestId(`pile-column-${id}`)).toBeDefined();
    }
    expect(within(chrome).getByTestId('hud-life-p0').textContent).toBe('18');
  });

  it('reconstructs the public board from one view (mid-game join)', () => {
    // A fresh mount from a single SpectatorView renders the board — no history needed.
    render(<SpectatorTable view={spectatorView()} />);
    // The public permanent is inspectable (a read-only surface exists for it).
    expect(screen.getByTestId('inspect-surface-perm_1')).toBeDefined();
    // The opponent's public graveyard pile is browsable on the board.
    expect(screen.getByTestId('table-graveyard-p0')).toBeDefined();
  });

  it('browses a public graveyard from its board pile', () => {
    render(<SpectatorTable view={spectatorView()} />);
    fireEvent.click(screen.getByTestId('table-graveyard-p0'));
    const browser = screen.getByTestId('zone-browser');
    expect(within(browser).getByTestId('zone-browser-title').textContent).toContain('Graveyard');
    expect(within(browser).getByTestId('browser-card-gy_0').textContent).toContain('Shock');
  });

  it('pins the inspect popover from a card’s read-only surface', () => {
    render(<SpectatorTable view={spectatorView()} />);
    fireEvent.click(screen.getByTestId('inspect-surface-perm_1'));
    expect(screen.getByTestId('card-inspect-name').textContent).toBe('Grizzly Bears');
  });

  it('docks the rail with the quiet empty-stack state and the log', () => {
    render(<SpectatorTable view={spectatorView()} />);
    // The rail is carved into the shell: both sections present, the empty stack
    // showing its designed quiet state rather than vanishing.
    const rail = screen.getByTestId('rail');
    expect(within(rail).getByTestId('rail-stack')).toBeDefined();
    expect(within(rail).getByTestId('rail-activity')).toBeDefined();
    expect(within(rail).getByTestId('stack-quiet')).toBeDefined();
    expect(within(rail).getByTestId('game-log')).toBeDefined();
  });

  it('shows a populated stack and log in the rail', () => {
    render(
      <SpectatorTable
        view={spectatorView({
          stack: [{ id: 's1', controller: 'p0', description: 'Lightning Bolt' }],
          log: [
            {
              sequence: 1,
              event: {
                type: 'spell_cast',
                player: 'p0',
                card: { id: 's1', name: 'Lightning Bolt' },
              },
            },
          ],
        })}
      />,
    );
    const rail = screen.getByTestId('rail');
    expect(within(rail).getByTestId('stack-item-s1').textContent).toContain('Lightning Bolt');
    expect(within(rail).getByTestId('log-entry-1').textContent).toBe('p0 cast Lightning Bolt.');
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
