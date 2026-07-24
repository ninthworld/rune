import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { normalizeSpectatorView } from '../wire';
import type { SpectatorView } from '../protocol';
import { useGameStore } from '../store';
import { SpectatorTable } from './SpectatorTable';

// The passive effects overlay is Pixi/WebGL; the spectator composition rides the
// same live stack players do, so the same headless mocks apply (issue #504).
vi.mock('./EffectsSurface', () => ({
  EffectsSurface: () => <div data-testid="effects-surface" aria-hidden="true" />,
}));
vi.mock('./effects', () => ({
  EffectsLayer: class {
    setPersistent(): void {}
    replaceTransients(): void {}
    trackMotion(): void {}
  },
}));

/** A live 3-seat spectator view: one eliminated seat, a permanent on the board, and
 * a public graveyard pile — the read fixture the spectate-mode tests build on. */
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

// Stage at the desktop reference geometry (1280×800, the same figure `useViewport`
// falls back to under SSR). jsdom's incidental 1024×768 default sits below the
// tablet floor (`compactFloorWidth` 1180, #500), which changes the staging kind to
// the compact summary-tile branch — not the full desktop staging these tests assert.
const originalInnerWidth = window.innerWidth;
const originalInnerHeight = window.innerHeight;
beforeEach(() => {
  Object.defineProperty(window, 'innerWidth', { value: 1280, configurable: true, writable: true });
  Object.defineProperty(window, 'innerHeight', { value: 800, configurable: true, writable: true });
  vi.spyOn(window, 'requestAnimationFrame').mockReturnValue(1);
  vi.spyOn(window, 'cancelAnimationFrame').mockImplementation(() => undefined);
});
afterEach(() => {
  cleanup();
  useGameStore.setState({ spectatorView: null, sessionEpoch: 0 });
  vi.restoreAllMocks();
  Object.defineProperty(window, 'innerWidth', {
    value: originalInnerWidth,
    configurable: true,
    writable: true,
  });
  Object.defineProperty(window, 'innerHeight', {
    value: originalInnerHeight,
    configurable: true,
    writable: true,
  });
});

describe('SpectatorTable (ADR 0022 / ADR 0030 plane, issue #504)', () => {
  it('renders the read-only fixed shell on the live plane: top bar, badge, effects', () => {
    render(<SpectatorTable view={spectatorView()} />);
    expect(screen.getByTestId('spectator-table')).toBeDefined();
    expect(screen.getByTestId('top-bar')).toBeDefined();
    expect(screen.getByTestId('live-2-5d-plane')).toBeDefined();
    expect(screen.getByTestId('effects-surface')).toBeDefined();
    expect(screen.getByTestId('spectator-badge').textContent).toContain('Spectating');
    // No canvas card renderer survives; Pixi is effects-only.
    expect(document.querySelector('canvas')).toBeNull();
  });

  it('offers no action affordances: a public permanent is inspect-only', () => {
    render(<SpectatorTable view={spectatorView()} />);
    // Nothing is playable or targetable — the permanent carries only its
    // transparent inspect surface (the live control layer degrades to inspect
    // because the public view has no valid_actions and no candidates).
    expect(screen.queryByTestId('entity-perm_1')).toBeNull();
    expect(screen.queryByTestId('target-perm_1')).toBeNull();
    expect(screen.getByTestId('inspect-surface-perm_1')).toBeDefined();
    // No hand, no action dock.
    expect(screen.queryByTestId('live-hand')).toBeNull();
    expect(screen.queryByTestId('action-bar')).toBeNull();
  });

  it('stages every seat as an opponent with a read-only focus control', () => {
    render(<SpectatorTable view={spectatorView()} />);
    // Receiver-less staging: every seat gets a focus-seat control, and focus
    // switching is a live-only presentation change (no action fires).
    for (const id of ['p0', 'p1', 'p2']) {
      expect(screen.getByTestId(`focus-seat-${id}`)).toBeDefined();
    }
    fireEvent.click(screen.getByTestId('focus-seat-p2'));
    // The board is still fully reconstructed after a focus switch.
    expect(screen.getByTestId('inspect-surface-perm_1')).toBeDefined();
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

  it('rebuilds the complete board through the reconnect path and flashes the cue', async () => {
    act(() => {
      useGameStore.setState({ sessionEpoch: 1 });
    });
    const { rerender } = render(<SpectatorTable view={spectatorView()} />);
    const shell = screen.getByTestId('spectator-table');
    expect(shell.getAttribute('data-orienting')).toBeNull();

    // A transport-generation discontinuity plus a fresh frame: the same
    // rebuild()/skipTransitions() path players use.
    act(() => {
      useGameStore.setState((state) => ({ sessionEpoch: state.sessionEpoch + 1 }));
    });
    rerender(<SpectatorTable view={spectatorView()} />);

    // The complete latest board is present from the one reconnect view…
    expect(screen.getByTestId('inspect-surface-perm_1')).toBeDefined();
    expect(shell.getAttribute('data-orienting')).toBe('true');
    // …and the cue is non-blocking: it retires itself.
    await waitFor(() => expect(shell.getAttribute('data-orienting')).toBeNull());
  });
});
