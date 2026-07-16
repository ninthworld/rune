import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import {
  BOTTOM_GAME_VIEW_JSON,
  DECLARE_ATTACKERS_GAME_VIEW_JSON,
  DECLARE_BLOCKERS_GAME_VIEW_JSON,
  DISCARD_GAME_VIEW_JSON,
  GAME_OVER_DRAW_JSON,
  GAME_OVER_LOSS_JSON,
  GAME_OVER_WIN_JSON,
  MULLIGAN_GAME_VIEW_JSON,
  OPTION_GAME_VIEW_JSON,
  ORDER_GAME_VIEW_JSON,
  SAMPLE_GAME_VIEW_JSON,
  TARGETING_GAME_VIEW_JSON,
  ZONE_SELECT_GAME_VIEW_JSON,
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

    // Our own tile shows our own life and library size (issue #255) — a player can
    // read their own life, not only their opponents'.
    expect(within(screen.getByTestId('tile-p1')).getByText(/Life 18/)).toBeDefined();
    expect(within(screen.getByTestId('tile-p1')).getByText(/Library 52/)).toBeDefined();

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

describe('Table stack panel (issue #142)', () => {
  it('renders the stack panel from GameView.stack on a live view', () => {
    seed(SAMPLE_GAME_VIEW_JSON);
    render(<Table />);
    // The sample frame carries one spell on the stack (s1 Lightning Bolt).
    expect(screen.getByTestId('stack-panel')).toBeDefined();
    expect(screen.getByTestId('stack-item-s1').textContent).toContain('Lightning Bolt');
  });

  it('shows no stack panel when the stack is empty', () => {
    // A replacement frame with an empty stack removes the panel entirely.
    const emptyStack = JSON.stringify({
      you: 'p1',
      my_hand: [],
      opponents: [{ player_id: 'p2', hand_size: 2, life: 7, library_size: 30, graveyard_size: 5 }],
      battlefield: [],
      stack: [],
      phase: 'end',
      valid_actions: [],
    });
    seed(emptyStack);
    render(<Table />);
    expect(screen.queryByTestId('stack-panel')).toBeNull();
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

describe('Table multi-select: declare attackers (issue #143)', () => {
  let choose: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    choose = seed(DECLARE_ATTACKERS_GAME_VIEW_JSON);
    render(<Table />);
  });

  /** Open the attackers declaration from its subject-less action-bar button. */
  function enter(): void {
    fireEvent.click(
      within(screen.getByTestId('action-bar')).getByRole('button', { name: 'Declare attackers' }),
    );
  }

  it('opens multi-select (not a submit) and highlights exactly the candidates', () => {
    enter();
    expect(choose).not.toHaveBeenCalled();
    expect(screen.getByTestId('multiselect-prompt').textContent).toContain('Choose attackers');
    // Both eligible attackers are toggleable; nothing else is.
    expect(screen.getByTestId('target-atk_1')).toBeDefined();
    expect(screen.getByTestId('target-atk_2')).toBeDefined();
    expect(screen.queryByTestId('entity-atk_1')).toBeNull();
  });

  it('toggles a subset and confirms it atomically with the token', () => {
    enter();
    fireEvent.click(screen.getByTestId('target-atk_1'));
    fireEvent.click(screen.getByTestId('target-atk_2'));
    // Toggling reflects a running count in the banner.
    expect(screen.getByTestId('multiselect-count').textContent).toContain('2 selected');

    fireEvent.click(screen.getByTestId('multiselect-confirm'));
    expect(choose).toHaveBeenCalledTimes(1);
    const [action, targets] = choose.mock.calls[0] as [ValidAction, TargetChoice[]];
    expect(action.id).toBe('a5');
    expect(action.token).toBe('h:atk0');
    expect(targets).toEqual([{ slot: 'attackers', chosen: ['atk_1', 'atk_2'] }]);
  });

  it('allows the empty declaration (confirm with no attackers)', () => {
    enter();
    // Subset slots are always confirmable — the empty set legally declares none.
    fireEvent.click(screen.getByTestId('multiselect-confirm'));
    const [, targets] = choose.mock.calls[0] as [ValidAction, TargetChoice[]];
    expect(targets).toEqual([{ slot: 'attackers', chosen: [] }]);
  });

  it('untoggles a candidate on a second click', () => {
    enter();
    fireEvent.click(screen.getByTestId('target-atk_1'));
    fireEvent.click(screen.getByTestId('target-atk_1'));
    fireEvent.click(screen.getByTestId('multiselect-confirm'));
    const [, targets] = choose.mock.calls[0] as [ValidAction, TargetChoice[]];
    expect(targets).toEqual([{ slot: 'attackers', chosen: [] }]);
  });

  it('cancels without submitting, restoring the neutral action bar', () => {
    enter();
    fireEvent.click(screen.getByTestId('multiselect-cancel'));
    expect(choose).not.toHaveBeenCalled();
    expect(screen.queryByTestId('target-atk_1')).toBeNull();
    expect(
      within(screen.getByTestId('action-bar')).getByRole('button', { name: 'Declare attackers' }),
    ).toBeDefined();
  });

  it('drops the in-progress selection when a fresh view arrives (changed token)', () => {
    enter();
    fireEvent.click(screen.getByTestId('target-atk_1'));
    act(() => useGameStore.getState().ingest(SAMPLE_GAME_VIEW_JSON));
    expect(screen.queryByTestId('target-atk_1')).toBeNull();
    expect(screen.queryByTestId('multiselect-prompt')).toBeNull();
    expect(choose).not.toHaveBeenCalled();
  });
});

describe('Table multi-select: declare blockers per-attacker (issue #143)', () => {
  let choose: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    choose = seed(DECLARE_BLOCKERS_GAME_VIEW_JSON);
    render(<Table />);
  });

  function enter(): void {
    fireEvent.click(
      within(screen.getByTestId('action-bar')).getByRole('button', { name: 'Declare blockers' }),
    );
  }

  it('walks one slot per attacker and assigns blockers per attacker', () => {
    enter();
    // First attacker's slot: both defenders are eligible to block it.
    expect(screen.getByTestId('multiselect-prompt').textContent).toContain('Verdant Scout');
    expect(screen.getByTestId('multiselect-step').textContent).toContain('Step 1 of 2');
    fireEvent.click(screen.getByTestId('target-blk_1'));
    fireEvent.click(screen.getByTestId('target-blk_2'));

    // Advance to the second attacker's slot; only one defender may block it.
    fireEvent.click(within(screen.getByTestId('action-bar')).getByRole('button', { name: 'Next' }));
    expect(screen.getByTestId('multiselect-prompt').textContent).toContain('Hill Giant');
    expect(screen.queryByTestId('target-blk_2')).toBeNull();
    fireEvent.click(screen.getByTestId('target-blk_1'));

    fireEvent.click(screen.getByTestId('multiselect-confirm'));
    const [action, targets] = choose.mock.calls[0] as [ValidAction, TargetChoice[]];
    expect(action.token).toBe('h:blk0');
    expect(targets).toEqual([
      { slot: 'block_atk_1', chosen: ['blk_1', 'blk_2'] },
      { slot: 'block_atk_2', chosen: ['blk_1'] },
    ]);
  });
});

describe('Table multi-select: mulligan bottoming (issue #143)', () => {
  let choose: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    choose = seed(MULLIGAN_GAME_VIEW_JSON);
    render(<Table />);
  });

  function enter(): void {
    fireEvent.click(
      within(screen.getByTestId('action-bar')).getByRole('button', { name: 'Keep or mulligan' }),
    );
  }

  it('renders the keep/mulligan options and the hand bottoming candidates', () => {
    enter();
    expect(screen.getByTestId('multiselect-option-keep')).toBeDefined();
    expect(screen.getByTestId('multiselect-option-mulligan')).toBeDefined();
    // The select_from_zone candidates are the hand cards.
    expect(screen.getByTestId('target-card_a')).toBeDefined();
    expect(screen.getByTestId('target-card_b')).toBeDefined();
    expect(screen.getByTestId('multiselect-count').textContent).toContain('0 of 1 selected');
  });

  it('keeps and bottoms the picked card in one atomic answer', () => {
    enter();
    fireEvent.click(screen.getByTestId('target-card_a'));
    fireEvent.click(screen.getByTestId('multiselect-option-keep'));
    expect(choose).toHaveBeenCalledTimes(1);
    const [action, targets] = choose.mock.calls[0] as [ValidAction, TargetChoice[]];
    expect(action.token).toBe('h:mull');
    expect(targets).toEqual([
      { slot: 'decision', chosen: ['keep'] },
      { slot: 'bottom', chosen: ['card_a'] },
    ]);
  });

  it('blocks the option buttons while the bottom pick exceeds the advertised count', () => {
    enter();
    // count is 1: picking a second card makes the selection invalid, disabling submit.
    fireEvent.click(screen.getByTestId('target-card_a'));
    fireEvent.click(screen.getByTestId('target-card_b'));
    expect(screen.getByTestId('multiselect-option-keep')).toHaveProperty('disabled', true);
    expect(screen.getByTestId('multiselect-option-mulligan')).toHaveProperty('disabled', true);
  });
});

describe('Table multi-select: select-from-zone count gate (issue #143)', () => {
  let choose: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    choose = seed(BOTTOM_GAME_VIEW_JSON);
    render(<Table />);
  });

  function enter(): void {
    fireEvent.click(
      within(screen.getByTestId('action-bar')).getByRole('button', { name: 'Keep hand' }),
    );
  }

  it('disables confirm until exactly the advertised count is picked', () => {
    enter();
    // Nothing picked: confirm is disabled (count is 2). This is a UX affordance only.
    expect(screen.getByTestId('multiselect-confirm')).toHaveProperty('disabled', true);
    fireEvent.click(screen.getByTestId('target-card_a'));
    expect(screen.getByTestId('multiselect-confirm')).toHaveProperty('disabled', true);

    fireEvent.click(screen.getByTestId('target-card_b'));
    expect(screen.getByTestId('multiselect-confirm')).toHaveProperty('disabled', false);

    fireEvent.click(screen.getByTestId('multiselect-confirm'));
    const [action, targets] = choose.mock.calls[0] as [ValidAction, TargetChoice[]];
    expect(action.token).toBe('h:keep');
    expect(targets).toEqual([{ slot: 'bottom', chosen: ['card_a', 'card_b'] }]);
  });
});

describe('Table option: modal picker in the banner (issue #157)', () => {
  let choose: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    choose = seed(OPTION_GAME_VIEW_JSON);
    render(<Table />);
  });

  function enter(): void {
    fireEvent.click(
      within(screen.getByTestId('action-bar')).getByRole('button', { name: 'Fork in the Road' }),
    );
  }

  it('renders the named choices as buttons in the prompt banner', () => {
    enter();
    expect(choose).not.toHaveBeenCalled();
    const banner = screen.getByTestId('prompt-banner');
    // The modal option picker lives in the banner (issue #157), not the action bar.
    expect(within(banner).getByTestId('multiselect-option-draw')).toBeDefined();
    expect(within(banner).getByTestId('multiselect-option-gain')).toBeDefined();
    expect(screen.getByTestId('multiselect-options').textContent).toContain('Choose a mode');
    // A pure option decision shows no selection count.
    expect(screen.queryByTestId('multiselect-count')).toBeNull();
  });

  it('submits the chosen option id atomically with the content-binding token', () => {
    enter();
    fireEvent.click(screen.getByTestId('multiselect-option-gain'));
    expect(choose).toHaveBeenCalledTimes(1);
    const [action, targets] = choose.mock.calls[0] as [ValidAction, TargetChoice[]];
    expect(action.id).toBe('a8');
    expect(action.token).toBe('h:mode');
    expect(targets).toEqual([{ slot: 'mode', chosen: ['gain'] }]);
  });
});

describe('Table order: arrange list (issue #157)', () => {
  let choose: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    choose = seed(ORDER_GAME_VIEW_JSON);
    render(<Table />);
  });

  function enter(): void {
    fireEvent.click(
      within(screen.getByTestId('action-bar')).getByRole('button', { name: 'Order triggers' }),
    );
  }

  it('opens the reorder surface with every item in the server order', () => {
    enter();
    expect(choose).not.toHaveBeenCalled();
    const surface = screen.getByTestId('prompt-surface');
    // Each ordered item is labelled by its card name.
    expect(within(surface).getByText('Soul Warden')).toBeDefined();
    expect(within(surface).getByText('Ajani’s Welcome')).toBeDefined();
    expect(within(surface).getByText('Impassioned Orator')).toBeDefined();
    // The first item cannot move up; the last cannot move down (clamped controls).
    expect(screen.getByTestId('order-up-trig_a')).toHaveProperty('disabled', true);
    expect(screen.getByTestId('order-down-trig_c')).toHaveProperty('disabled', true);
    // Order is always a complete permutation, so confirm is enabled immediately.
    expect(screen.getByTestId('multiselect-confirm')).toHaveProperty('disabled', false);
  });

  it('reorders items and submits the permutation with the token', () => {
    enter();
    // Move the last item (Impassioned Orator) up one: a,b,c → a,c,b.
    fireEvent.click(screen.getByTestId('order-up-trig_c'));
    fireEvent.click(screen.getByTestId('multiselect-confirm'));

    expect(choose).toHaveBeenCalledTimes(1);
    const [action, targets] = choose.mock.calls[0] as [ValidAction, TargetChoice[]];
    expect(action.token).toBe('h:ord0');
    expect(targets).toEqual([{ slot: 'order', chosen: ['trig_a', 'trig_c', 'trig_b'] }]);
  });

  it('reconstructs the identical order surface from a replayed view (rehydration)', () => {
    enter();
    // Reorder mid-prompt, then replay the same view (a refresh/reconnect resend).
    fireEvent.click(screen.getByTestId('order-up-trig_c'));
    act(() => useGameStore.getState().ingest(ORDER_GAME_VIEW_JSON));

    // The ephemeral session is dropped (no state across messages); the surface is
    // gone and the action is offered again, so the prompt is fully reconstructable.
    expect(screen.queryByTestId('prompt-surface')).toBeNull();
    expect(choose).not.toHaveBeenCalled();

    // Re-opening rebuilds the identical surface in the server's initial order —
    // the earlier reorder left no residue.
    enter();
    fireEvent.click(screen.getByTestId('multiselect-confirm'));
    const [, targets] = choose.mock.calls[0] as [ValidAction, TargetChoice[]];
    expect(targets).toEqual([{ slot: 'order', chosen: ['trig_a', 'trig_b', 'trig_c'] }]);
  });
});

describe('Table select-from-zone: non-visible zone overlay (issue #157)', () => {
  let choose: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    choose = seed(ZONE_SELECT_GAME_VIEW_JSON);
    render(<Table />);
  });

  function enter(): void {
    fireEvent.click(
      within(screen.getByTestId('action-bar')).getByRole('button', {
        name: 'Return a card to hand',
      }),
    );
  }

  it('surfaces graveyard candidates in the overlay list, not on the canvas', () => {
    enter();
    const surface = screen.getByTestId('prompt-surface');
    expect(within(surface).getByText('Llanowar Elves')).toBeDefined();
    expect(within(surface).getByTestId('zone-select-gy_2')).toBeDefined();
    // The graveyard is not on the board, so there is no canvas target hotspot.
    expect(screen.queryByTestId('target-gy_2')).toBeNull();
  });

  it('count-gates confirm and submits the picked id atomically with the token', () => {
    enter();
    expect(screen.getByTestId('multiselect-confirm')).toHaveProperty('disabled', true);
    fireEvent.click(screen.getByTestId('zone-select-gy_2'));
    expect(screen.getByTestId('multiselect-confirm')).toHaveProperty('disabled', false);

    fireEvent.click(screen.getByTestId('multiselect-confirm'));
    const [action, targets] = choose.mock.calls[0] as [ValidAction, TargetChoice[]];
    expect(action.token).toBe('h:gy0');
    expect(targets).toEqual([{ slot: 'return', chosen: ['gy_2'] }]);
  });
});

describe('Table discard-to-max end to end (issue #156/#157)', () => {
  let choose: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    choose = seed(DISCARD_GAME_VIEW_JSON);
    render(<Table />);
  });

  function enter(): void {
    fireEvent.click(
      within(screen.getByTestId('action-bar')).getByRole('button', {
        name: 'Discard to hand size',
      }),
    );
  }

  it('highlights hand cards in place (not the overlay) and submits the discard', () => {
    enter();
    // The hand IS on the board, so candidates highlight in place — no overlay list.
    expect(screen.queryByTestId('prompt-surface')).toBeNull();
    expect(screen.getByTestId('target-h8')).toBeDefined();
    expect(screen.getByTestId('multiselect-count').textContent).toContain('0 of 1 selected');

    // Confirm is count-gated: the 8th card must be chosen to complete cleanup.
    expect(screen.getByTestId('multiselect-confirm')).toHaveProperty('disabled', true);
    fireEvent.click(screen.getByTestId('target-h8'));
    expect(screen.getByTestId('multiselect-confirm')).toHaveProperty('disabled', false);

    fireEvent.click(screen.getByTestId('multiselect-confirm'));
    const [action, targets] = choose.mock.calls[0] as [ValidAction, TargetChoice[]];
    expect(action.token).toBe('h:disc');
    expect(targets).toEqual([{ slot: 'discard', chosen: ['h8'] }]);
  });

  it('cancels the discard with Escape (keyboard parity with targeting)', () => {
    enter();
    expect(screen.getByTestId('multiselect-prompt')).toBeDefined();
    fireEvent.keyDown(window, { key: 'Escape' });
    // The selection is abandoned with nothing submitted; the neutral bar returns.
    expect(choose).not.toHaveBeenCalled();
    expect(screen.queryByTestId('multiselect-prompt')).toBeNull();
    expect(
      within(screen.getByTestId('action-bar')).getByRole('button', {
        name: 'Discard to hand size',
      }),
    ).toBeDefined();
  });
});
