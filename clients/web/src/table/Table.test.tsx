import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import {
  GAME_OVER_DRAW_JSON,
  GAME_OVER_LOSS_JSON,
  GAME_OVER_WIN_JSON,
  SAMPLE_GAME_VIEW_JSON,
  TARGETING_GAME_VIEW_JSON,
} from '../game-view.fixture';
import type { TargetChoice, ValidAction } from '../protocol';
import { useGameStore } from '../store';
import { Table } from './Table';

/**
 * The routing tests drive the real store singleton (feeding it a lone GameView,
 * exactly the reconstruct-from-one-GameView seam) and spy on `choose`, so we
 * assert the id echoed back rather than any socket traffic.
 */
function seed(json: string): ReturnType<typeof vi.fn> {
  const choose = vi.fn();
  useGameStore.getState().ingest(json);
  useGameStore.setState({ choose });
  return choose;
}

afterEach(() => {
  cleanup();
  useGameStore.setState({ view: null });
});

describe('Table action routing (ADR 0004)', () => {
  let choose: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    choose = seed(SAMPLE_GAME_VIEW_JSON);
    render(<Table />);
  });

  it('keeps per-card actions out of the bar until the entity is selected', () => {
    const bar = screen.getByTestId('action-bar');
    // Only the global action shows; the entity action is not enumerated here.
    expect(within(bar).getByRole('button', { name: 'Pass' })).toBeDefined();
    expect(within(bar).queryByRole('button', { name: 'Tap for mana' })).toBeNull();
  });

  it('fires an entity action from the entity (select-then-confirm)', () => {
    // Select the permanent via its on-entity hotspot...
    fireEvent.click(screen.getByTestId('entity-perm_xyz'));
    // ...then confirm the action rendered ON the entity.
    const onEntity = screen.getByTestId('entity-actions-perm_xyz');
    fireEvent.click(within(onEntity).getByRole('button', { name: 'Tap for mana' }));

    expect(choose).toHaveBeenCalledTimes(1);
    // The store receives the whole ValidAction (it reads id + token); no targets.
    expect((choose.mock.calls[0][0] as ValidAction).id).toBe('a2');
    expect(choose.mock.calls[0][1]).toBeUndefined();
  });

  it('echoes the selection into the bar and fires from there too', () => {
    fireEvent.click(screen.getByTestId('entity-perm_xyz'));
    const echo = screen.getByTestId('selection-echo');
    fireEvent.click(within(echo).getByRole('button', { name: 'Tap for mana' }));
    expect((choose.mock.calls[0][0] as ValidAction).id).toBe('a2');
  });

  it('fires a global action from the bar', () => {
    const bar = screen.getByTestId('action-bar');
    fireEvent.click(within(bar).getByRole('button', { name: 'Pass' }));
    expect(choose).toHaveBeenCalledTimes(1);
    expect((choose.mock.calls[0][0] as ValidAction).id).toBe('a1');
  });

  it('offers no hotspot for a card without a valid action', () => {
    // The hand card c1 has no subject-action, so it is not interactive.
    expect(screen.queryByTestId('entity-c1')).toBeNull();
  });
});

describe('Table reconstructs from one GameView (reconnect/replay)', () => {
  it('rebuilds the whole UI from a replacement frame with no residue', () => {
    seed(SAMPLE_GAME_VIEW_JSON);
    render(<Table />);

    // First frame: opponent p2 at 20 life, our Grizzly Bears is interactive.
    expect(within(screen.getByTestId('tile-p2')).getByText(/Life 20/)).toBeDefined();
    expect(screen.getByTestId('entity-perm_xyz')).toBeDefined();

    // A fresh frame replaces everything — as a reconnect would.
    const next = JSON.stringify({
      my_hand: [],
      opponents: [{ player_id: 'p2', hand_size: 2, life: 7, library_size: 30, graveyard_size: 5 }],
      battlefield: [],
      phase: 'end',
      valid_actions: [],
    });
    act(() => useGameStore.getState().ingest(next));

    // The UI reflects only the new frame: updated life, no stale entity, and the
    // action bar is empty (input gated: no valid_actions).
    expect(within(screen.getByTestId('tile-p2')).getByText(/Life 7/)).toBeDefined();
    expect(screen.queryByTestId('entity-perm_xyz')).toBeNull();
    expect(
      within(screen.getByTestId('action-bar')).getByText('No actions available'),
    ).toBeDefined();
    expect(screen.getByTestId('prompt-banner').textContent).toContain('Waiting');
  });
});

describe('Table game-over (issue #141)', () => {
  it('renders the game-over overlay and suppresses the prompt/action UI on a terminal view', () => {
    seed(GAME_OVER_WIN_JSON);
    render(<Table />);

    // The DOM overlay is shown, naming the receiver's victory.
    expect(screen.getByTestId('game-over-overlay')).toBeDefined();
    expect(screen.getByTestId('game-over-headline').textContent).toBe('Victory');
    // Prompt banner and action bar are suppressed once the game is over.
    expect(screen.queryByTestId('prompt-banner')).toBeNull();
    expect(screen.queryByTestId('action-bar')).toBeNull();
  });

  it('phrases a loss from the receiver’s seat', () => {
    seed(GAME_OVER_LOSS_JSON);
    render(<Table />);
    expect(screen.getByTestId('game-over-headline').textContent).toBe('Defeat');
  });

  it('phrases a draw', () => {
    seed(GAME_OVER_DRAW_JSON);
    render(<Table />);
    expect(screen.getByTestId('game-over-headline').textContent).toBe('Draw');
  });

  it('shows no overlay while the game is live (non-terminal view)', () => {
    seed(SAMPLE_GAME_VIEW_JSON);
    render(<Table />);
    expect(screen.queryByTestId('game-over-overlay')).toBeNull();
    expect(screen.getByTestId('action-bar')).toBeDefined();
  });

  it('reconstructs the same screen from the terminal view alone (reconnect/replay)', () => {
    // Drive a live view, then replace it wholesale with the terminal frame — as a
    // refresh + reconnect that replays the final view would. The overlay is pure
    // render of the latest view, so the result is identical to seeding it directly.
    seed(SAMPLE_GAME_VIEW_JSON);
    render(<Table />);
    expect(screen.queryByTestId('game-over-overlay')).toBeNull();

    act(() => useGameStore.getState().ingest(GAME_OVER_WIN_JSON));
    expect(screen.getByTestId('game-over-overlay')).toBeDefined();
    expect(screen.getByTestId('game-over-headline').textContent).toBe('Victory');
    expect(screen.queryByTestId('action-bar')).toBeNull();
  });
});

describe('Table targeting mode (ADR 0009 §Client)', () => {
  let choose: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    choose = seed(TARGETING_GAME_VIEW_JSON);
    render(<Table />);
  });

  /** Enter targeting: select the spell card, then confirm its cast action. */
  function enterTargeting(): void {
    fireEvent.click(screen.getByTestId('entity-c3'));
    const onEntity = screen.getByTestId('entity-actions-c3');
    fireEvent.click(within(onEntity).getByRole('button', { name: 'Cast Lightning Bolt' }));
  }

  it('does not submit when a targeted action is chosen — it opens targeting mode', () => {
    enterTargeting();
    // No ChooseAction yet: the answer is only sent once targets are picked.
    expect(choose).not.toHaveBeenCalled();
    // The banner announces the server-provided target prompt.
    expect(screen.getByTestId('targeting-prompt').textContent).toContain(
      'target creature or player',
    );
  });

  it('highlights exactly the server candidates and makes nothing else pickable', () => {
    enterTargeting();
    // The two server-listed candidates are pickable: the permanent and the player.
    expect(screen.getByTestId('target-perm_xyz')).toBeDefined();
    expect(screen.getByTestId('target-player-p2')).toBeDefined();
    // The spell card itself is NOT a candidate, so it has no target hotspot, and
    // the normal action hotspots are gone (targeting suppresses them).
    expect(screen.queryByTestId('target-c3')).toBeNull();
    expect(screen.queryByTestId('entity-c3')).toBeNull();
    expect(screen.queryByTestId('entity-perm_xyz')).toBeNull();
  });

  it('submits atomically with the content-binding token when a permanent is picked', () => {
    enterTargeting();
    fireEvent.click(screen.getByTestId('target-perm_xyz'));

    expect(choose).toHaveBeenCalledTimes(1);
    const [action, targets] = choose.mock.calls[0] as [ValidAction, TargetChoice[]];
    // The whole action is passed (store echoes its token verbatim)...
    expect(action.id).toBe('a3');
    expect(action.token).toBe('h:9f2c');
    // ...along with one target choice per requirement slot, in a single call.
    expect(targets).toEqual([{ slot: 't0', chosen: ['perm_xyz'] }]);
  });

  it('can target a player by picking their portrait tile', () => {
    enterTargeting();
    fireEvent.click(screen.getByTestId('target-player-p2'));

    expect(choose).toHaveBeenCalledTimes(1);
    const [, targets] = choose.mock.calls[0] as [ValidAction, TargetChoice[]];
    expect(targets).toEqual([{ slot: 't0', chosen: ['p2'] }]);
  });

  it('cancels targeting without submitting, restoring normal interactivity', () => {
    enterTargeting();
    fireEvent.click(
      within(screen.getByTestId('action-bar')).getByRole('button', { name: 'Cancel targeting' }),
    );

    expect(choose).not.toHaveBeenCalled();
    // Back to normal: the spell card is selectable again, no target hotspots.
    expect(screen.queryByTestId('target-perm_xyz')).toBeNull();
    expect(screen.getByTestId('entity-c3')).toBeDefined();
  });

  it('drops in-progress targeting when a fresh GameView arrives (no state across messages)', () => {
    enterTargeting();
    expect(screen.getByTestId('target-perm_xyz')).toBeDefined();

    // A new frame supersedes the pending decision; targeting must reset so the UI
    // is reconstructable from the new view alone.
    act(() => useGameStore.getState().ingest(SAMPLE_GAME_VIEW_JSON));
    expect(screen.queryByTestId('target-perm_xyz')).toBeNull();
    expect(screen.queryByTestId('targeting-prompt')).toBeNull();
    expect(choose).not.toHaveBeenCalled();
  });
});
