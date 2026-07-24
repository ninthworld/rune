import { beforeEach, describe, expect, it, vi } from 'vitest';
import { act, fireEvent, render, screen, within } from '@testing-library/react';
import { SAMPLE_GAME_VIEW_JSON, TARGETING_GAME_VIEW_JSON } from '../game-view.fixture';
import type { TargetChoice, ValidAction } from '../protocol';
import { useGameStore } from '../store';
import { Table } from './Table';
import { registerTableTestHooks, seed } from './table-test-support';

registerTableTestHooks();

describe('Table targeting mode (ADR 0009 §Client)', () => {
  let choose: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    choose = seed(TARGETING_GAME_VIEW_JSON);
    render(<Table />);
  });

  /** Enter targeting: select the spell card, then confirm its cast action from the
   * dock — the one action home the selection routes to (ADR 0023). */
  function enterTargeting(): void {
    fireEvent.click(screen.getByTestId('entity-c3'));
    const echo = screen.getByTestId('selection-echo');
    fireEvent.click(within(echo).getByRole('button', { name: 'Cast Lightning Bolt' }));
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
