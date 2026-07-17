/**
 * Player HUD surfaces (issue #296): the opponent strip and the local dock that
 * replace the uniform text tiles. This covers the rendered chrome and the preserved
 * targeting contract; the strip's geometric reflow (region rects by seat count) is
 * covered in layout.test.ts. Together they satisfy the acceptance list: a 2p layout
 * shows one opponent tile + the dock, an 8p fixture reflows the strip to seven tiles
 * without the local player leaving the dock, targeting rings/dims exactly the server
 * candidates, and no hidden-zone count is repeated on either surface.
 */
import { render, screen, fireEvent, cleanup, within } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { OpponentHud, LocalDock } from './PlayerHud';
import { normalizeGameView } from '../wire';
import type { GameView } from '../protocol';

/** A minimal live view with `seats` opponents (p2…), the receiver p1, and names. */
function viewWith(seats: number, extra: Partial<Record<string, unknown>> = {}): GameView {
  const opponents = Array.from({ length: seats }, (_, i) => ({
    player_id: `p${i + 2}`,
    hand_size: 3 + i,
    life: 20 - i,
    library_size: 40,
    graveyard_size: 0,
    statuses: i === 0 ? ['monarch'] : undefined,
  }));
  const player_names: Record<string, string> = { p1: 'You-Name' };
  opponents.forEach((o, i) => (player_names[o.player_id] = `Foe ${i + 1}`));
  return normalizeGameView({
    you: 'p1',
    my_hand: [],
    me: { life: 17, library_size: 42 },
    opponents,
    battlefield: [],
    phase: 'precombat_main',
    valid_actions: [],
    mana_pool: ['{G}', '{U}'],
    player_names,
    ...extra,
  });
}

describe('OpponentHud (issue #296)', () => {
  afterEach(cleanup);

  it('renders one tile per opponent with identity and life, and secondary meta', () => {
    render(<OpponentHud view={viewWith(1)} />);
    // Exactly one opponent tile at 2 players (one receiver + one opponent).
    expect(screen.getAllByTestId(/^tile-p/)).toHaveLength(1);
    expect(screen.getByTestId('hud-name-p2').textContent).toBe('Foe 1');
    // Life is rendered verbatim; hand count and statuses are the secondary meta.
    expect(screen.getByTestId('hud-life-p2').textContent).toBe('20');
    expect(screen.getByTestId('hud-hand-p2').textContent).toContain('3');
    expect(screen.getByTestId('hud-statuses-p2').textContent).toContain('monarch');
  });

  it('reflows to one tile per opponent at 8 players without a local tile in the strip', () => {
    render(<OpponentHud view={viewWith(7)} />);
    // 8 players → 7 opponent tiles in the strip; the receiver (p1) is NOT here.
    expect(screen.getAllByTestId(/^tile-p/)).toHaveLength(7);
    expect(screen.queryByTestId('tile-p1')).toBeNull();
  });

  it('repeats no hidden-zone count (library/graveyard/exile live on the board piles)', () => {
    render(<OpponentHud view={viewWith(1)} />);
    const strip = screen.getByTestId('opponent-hud');
    expect(within(strip).queryByText(/Library/)).toBeNull();
    expect(within(strip).queryByText(/Graveyard/)).toBeNull();
    expect(within(strip).queryByText(/Exile/)).toBeNull();
  });

  it('makes a server-listed candidate pickable and dims every non-candidate (targeting)', () => {
    const onPick = vi.fn();
    // Two opponents; only p3 is a candidate for the active slot.
    render(<OpponentHud view={viewWith(2)} targeting={{ candidates: ['p3'], onPick }} />);
    // The candidate is a real pick button (≥44px enforced by the .targetTile class);
    // the non-candidate is an inert, dimmed div (still a tile, never a button).
    const pick = screen.getByTestId('target-player-p3');
    expect(pick.tagName).toBe('BUTTON');
    expect(screen.queryByTestId('target-player-p2')).toBeNull();
    expect(screen.getByTestId('tile-p2')).toBeDefined();
    fireEvent.click(pick);
    expect(onPick).toHaveBeenCalledWith('p3');
  });
});

describe('LocalDock (issue #296)', () => {
  afterEach(cleanup);

  it('shows the receiver identity and their own life, marked as you', () => {
    render(<LocalDock view={viewWith(1)} localId="p1" />);
    const dock = screen.getByTestId('local-dock');
    expect(screen.getByTestId('hud-name-p1').textContent).toContain('You-Name');
    expect(within(dock).getByText('(you)')).toBeDefined();
    expect(screen.getByTestId('hud-life-p1').textContent).toBe('17');
  });

  it('shows floating mana only when the pool is non-empty', () => {
    const { rerender } = render(<LocalDock view={viewWith(1)} localId="p1" />);
    expect(screen.getByTestId('hud-mana').textContent).toContain('{G}');
    // An empty pool drops the mana row entirely (present only when non-empty).
    rerender(<LocalDock view={viewWith(1, { mana_pool: [] })} localId="p1" />);
    expect(screen.queryByTestId('hud-mana')).toBeNull();
  });

  it('repeats no hidden-zone count on the dock', () => {
    const dock = render(<LocalDock view={viewWith(1)} localId="p1" />).getByTestId('local-dock');
    expect(within(dock).queryByText(/Library/)).toBeNull();
    expect(within(dock).queryByText(/Graveyard/)).toBeNull();
    expect(within(dock).queryByText(/Exile/)).toBeNull();
  });

  it('lets the receiver be a self-target candidate with the same ring contract', () => {
    const onPick = vi.fn();
    render(
      <LocalDock view={viewWith(1)} localId="p1" targeting={{ candidates: ['p1'], onPick }} />,
    );
    const pick = screen.getByTestId('target-player-p1');
    expect(pick.tagName).toBe('BUTTON');
    fireEvent.click(pick);
    expect(onPick).toHaveBeenCalledWith('p1');
  });

  it('falls back to a seat-agnostic label when the server names no receiver', () => {
    // No localId and no name → the dock still renders under a stable "You" label and
    // a stable local key, never a raw id (playerNames fallback contract).
    render(<LocalDock view={viewWith(1)} />);
    expect(screen.getByTestId('tile-local')).toBeDefined();
    expect(screen.getByTestId('hud-name-local').textContent).toContain('You');
  });
});
