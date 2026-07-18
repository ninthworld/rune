import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import {
  BOTTOM_GAME_VIEW_JSON,
  DECLARE_ATTACKERS_GAME_VIEW_JSON,
  DECLARE_BLOCKERS_GAME_VIEW_JSON,
  DISCARD_GAME_VIEW_JSON,
  FOUR_PLAYER_GAME_VIEW_JSON,
  GAME_OVER_DRAW_JSON,
  GAME_OVER_LOSS_JSON,
  GAME_OVER_WIN_JSON,
  MULLIGAN_GAME_VIEW_JSON,
  OPTION_GAME_VIEW_JSON,
  ORDER_GAME_VIEW_JSON,
  SAMPLE_GAME_VIEW_JSON,
  TARGETING_GAME_VIEW_JSON,
  ZONE_SELECT_GAME_VIEW_JSON,
  ZONES_GAME_VIEW_JSON,
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

    // First frame: opponent p2 at 20 life (HUD strip), our Grizzly Bears interactive.
    expect(screen.getByTestId('hud-life-p2').textContent).toBe('20');
    expect(screen.getByTestId('entity-perm_xyz')).toBeDefined();

    // Our own dock shows our own life (issue #255/#296) — a player can read their own
    // life, not only their opponents'. The library count lives ONCE, on the board's
    // zone pile (issue #296); the dock no longer repeats it.
    expect(screen.getByTestId('hud-life-p1').textContent).toBe('18');
    // The library count lives ONCE, on the board's card-shaped zone pile (issue #319);
    // its zone name rides the pile's accessible label, and the dock never repeats it.
    const libraryPile = screen.getByTestId('library-pile-p1');
    expect(libraryPile.getAttribute('aria-label')).toBe('p1 (you) library (52)');
    expect(within(libraryPile).getByText('52')).toBeDefined();
    expect(within(screen.getByTestId('local-dock')).queryByText(/Library/)).toBeNull();

    // A fresh frame replaces everything — as a reconnect would.
    const next = JSON.stringify({
      my_hand: [],
      opponents: [{ player_id: 'p2', hand_size: 2, life: 7, library_size: 30, graveyard_size: 5 }],
      battlefield: [],
      phase: 'end',
      valid_actions: [],
    });
    act(() => useGameStore.getState().ingest(next));

    // The UI reflects only the new frame: updated life, no stale entity, and input is
    // gated (no valid_actions): the tray reads "waiting" quietly (issue #298) and no
    // anchored prompt overlay is staged.
    expect(screen.getByTestId('hud-life-p2').textContent).toBe('7');
    expect(screen.queryByTestId('entity-perm_xyz')).toBeNull();
    expect(
      within(screen.getByTestId('action-bar')).getByTestId('tray-waiting').textContent,
    ).toContain('Waiting');
    expect(screen.queryByTestId('prompt-overlay')).toBeNull();
    expect(screen.queryByTestId('prompt-banner')).toBeNull();
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

describe('Table card inspect (issues #261/#321)', () => {
  it('has no permanently visible per-card inspect handle (issue #321)', () => {
    seed(SAMPLE_GAME_VIEW_JSON);
    render(<Table />);
    // The retired per-card "i" handles are gone; inert cards carry only a transparent
    // inspect surface, and nothing paints a visible inspect control on a card.
    expect(screen.queryByTestId('inspect-c1')).toBeNull();
    expect(screen.queryByTestId('inspect-perm_xyz')).toBeNull();
    expect(screen.getByTestId('inspect-surface-c1')).toBeDefined();
  });

  it('pins a hand card via its inspect surface and shows its CardView content', () => {
    seed(SAMPLE_GAME_VIEW_JSON);
    render(<Table />);
    // The hand card c1 (Llanowar Elves) has no action, yet it is inspectable via its
    // transparent surface (invisible, but a real focusable button).
    expect(screen.queryByTestId('card-inspect')).toBeNull();
    const surface = screen.getByTestId('inspect-surface-c1');
    expect(surface.tagName).toBe('BUTTON');
    fireEvent.click(surface);
    const panel = screen.getByTestId('card-inspect');
    expect(within(panel).getByTestId('card-inspect-name').textContent).toBe('Llanowar Elves');
    expect(within(panel).getByTestId('card-inspect-rules').textContent).toContain('Add {G}');
    // Pinned, not a transient peek: it carries the close control and blocks the modal.
    expect(panel.getAttribute('data-transient')).toBeNull();
  });

  it('right-clicks an actionable permanent to pin its inspect, and closes again', () => {
    seed(SAMPLE_GAME_VIEW_JSON);
    render(<Table />);
    // perm_xyz carries an action, so it has a select hotspot (no surface); right-click
    // pins its inspect without disturbing the offered action.
    fireEvent.contextMenu(screen.getByTestId('entity-perm_xyz'));
    const panel = screen.getByTestId('card-inspect');
    expect(within(panel).getByTestId('card-inspect-name').textContent).toBe('Grizzly Bears');
    expect(within(panel).getByTestId('card-inspect-state').textContent).toContain('Tapped');
    fireEvent.click(screen.getByTestId('card-inspect-close'));
    expect(screen.queryByTestId('card-inspect')).toBeNull();
  });

  it('surfaces a transient preview when a card is selected (issue #321)', () => {
    seed(SAMPLE_GAME_VIEW_JSON);
    render(<Table />);
    // Selecting an actionable card also surfaces its preview in the consistent home —
    // as a non-blocking peek (data-transient), not the pinned modal.
    fireEvent.click(screen.getByTestId('entity-perm_xyz'));
    const preview = screen.getByTestId('card-inspect');
    expect(preview.getAttribute('data-transient')).toBe('true');
    expect(within(preview).getByTestId('card-inspect-name').textContent).toBe('Grizzly Bears');
    // The peek adds no close control and never blocks (no backdrop).
    expect(screen.queryByTestId('card-inspect-close')).toBeNull();
    expect(screen.queryByTestId('card-inspect-backdrop')).toBeNull();
  });

  it('inspects a stack object', () => {
    seed(SAMPLE_GAME_VIEW_JSON);
    render(<Table />);
    fireEvent.click(screen.getByTestId('inspect-s1'));
    expect(screen.getByTestId('card-inspect-name').textContent).toBe('Lightning Bolt');
  });

  it('is keyboard accessible: the inspect surface is a focusable button, Escape closes', () => {
    seed(SAMPLE_GAME_VIEW_JSON);
    render(<Table />);
    const surface = screen.getByTestId('inspect-surface-c1');
    // A real button: focusable, Enter/Space activate it natively → pins the panel.
    expect(surface.tagName).toBe('BUTTON');
    fireEvent.click(surface);
    expect(screen.getByTestId('card-inspect')).toBeDefined();
    // Escape dismisses the pinned panel (keyboard parity with the other overlays).
    fireEvent.keyDown(window, { key: 'Escape' });
    expect(screen.queryByTestId('card-inspect')).toBeNull();
  });

  it('drops an open panel when a fresh GameView arrives (no state across messages)', () => {
    seed(SAMPLE_GAME_VIEW_JSON);
    render(<Table />);
    fireEvent.contextMenu(screen.getByTestId('entity-perm_xyz'));
    expect(screen.getByTestId('card-inspect')).toBeDefined();
    act(() => useGameStore.getState().ingest(SAMPLE_GAME_VIEW_JSON));
    expect(screen.queryByTestId('card-inspect')).toBeNull();
  });

  it('keeps a card both inspectable and targetable during targeting mode', () => {
    const choose = seed(TARGETING_GAME_VIEW_JSON);
    render(<Table />);
    // Enter targeting on the bolt.
    fireEvent.click(screen.getByTestId('entity-c3'));
    fireEvent.click(
      within(screen.getByTestId('entity-actions-c3')).getByRole('button', {
        name: 'Cast Lightning Bolt',
      }),
    );
    // The candidate permanent is targetable AND still inspectable (right-click pins,
    // even mid-pick — only the hover/long-press peeks are suppressed while targeting).
    expect(screen.getByTestId('target-perm_xyz')).toBeDefined();
    fireEvent.contextMenu(screen.getByTestId('target-perm_xyz'));
    expect(screen.getByTestId('card-inspect-name').textContent).toBe('Grizzly Bears');
    // Inspecting did not submit the target; targeting is still live underneath.
    expect(choose).not.toHaveBeenCalled();
    fireEvent.click(screen.getByTestId('card-inspect-close'));
    fireEvent.click(screen.getByTestId('target-perm_xyz'));
    expect(choose).toHaveBeenCalledTimes(1);
  });

  it('inspects in the read-only game-over state', () => {
    // A terminal frame that still carries a permanent to inspect.
    const terminal = JSON.stringify({
      you: 'p1',
      my_hand: [],
      opponents: [{ player_id: 'p2', hand_size: 0, life: 0, library_size: 40, graveyard_size: 0 }],
      battlefield: [
        {
          id: 'perm_win',
          controller: 'p1',
          owner: 'p1',
          card: { id: 'perm_win', name: 'Grizzly Bears', type_line: 'Creature — Bear' },
        },
      ],
      phase: 'end',
      valid_actions: [],
      result: { winner: 'p1', losers: ['p2'], reason: 'life_zero' },
    });
    seed(terminal);
    render(<Table />);
    expect(screen.getByTestId('game-over-overlay')).toBeDefined();
    // No actions in the terminal frame → every card is inert and carries a surface.
    fireEvent.click(screen.getByTestId('inspect-surface-perm_win'));
    expect(screen.getByTestId('card-inspect-name').textContent).toBe('Grizzly Bears');
  });
});

describe('Table phase/turn indicator and modes (issue #267, #297)', () => {
  it('shows the compact indicator with turn, active player, and current step', () => {
    seed(SAMPLE_GAME_VIEW_JSON);
    render(<Table />);
    const indicator = screen.getByTestId('phase-indicator');
    expect(within(indicator).getByTestId('indicator-turn').textContent).toBe('Turn 5');
    // p1 is the receiver and the active player → "Your turn".
    expect(within(indicator).getByTestId('indicator-active').textContent).toBe('Your turn');
    // The current step is shown compactly (no always-on twelve-pill strip).
    expect(within(indicator).getByTestId('indicator-step').textContent).toBe('Main Phase 1');
    expect(within(indicator).queryByTestId('indicator-steps')).toBeNull();
  });

  it('sits in overview mode on a normal view and shifts to focus when targeting opens', () => {
    seed(TARGETING_GAME_VIEW_JSON);
    render(<Table />);
    // A castable-but-optional spell is not a forced decision → overview on mount.
    expect(screen.getByTestId('phase-indicator').getAttribute('data-mode')).toBe('overview');
    expect(screen.queryByTestId('indicator-decision')).toBeNull();

    // Entering targeting visibly shifts to focus treatment...
    fireEvent.click(screen.getByTestId('entity-c3'));
    fireEvent.click(
      within(screen.getByTestId('entity-actions-c3')).getByRole('button', {
        name: 'Cast Lightning Bolt',
      }),
    );
    expect(screen.getByTestId('phase-indicator').getAttribute('data-mode')).toBe('focus');
    expect(screen.getByTestId('indicator-decision')).toBeDefined();

    // ...and cancelling it returns to overview.
    fireEvent.click(
      within(screen.getByTestId('action-bar')).getByRole('button', { name: 'Cancel targeting' }),
    );
    expect(screen.getByTestId('phase-indicator').getAttribute('data-mode')).toBe('overview');
  });

  it('renders focus treatment directly from a fresh mid-prompt GameView (no history)', () => {
    // A declare-attackers view poses a forced, subject-less decision, so a fresh
    // mount lands in focus mode without any prior interaction.
    seed(DECLARE_ATTACKERS_GAME_VIEW_JSON);
    render(<Table />);
    expect(screen.getByTestId('phase-indicator').getAttribute('data-mode')).toBe('focus');
    expect(screen.getByTestId('indicator-decision')).toBeDefined();
  });

  it('renders the game-over state in overview treatment beneath the overlay', () => {
    seed(GAME_OVER_WIN_JSON);
    render(<Table />);
    expect(screen.getByTestId('game-over-overlay')).toBeDefined();
    expect(screen.getByTestId('table-game-over').getAttribute('data-mode')).toBe('overview');
    // The indicator is still visible in the terminal state.
    expect(screen.getByTestId('phase-indicator')).toBeDefined();
  });
});

describe('Table decision staging (issue #298)', () => {
  it('stages an anchored prompt overlay for a decision and tears it down on cancel', () => {
    seed(TARGETING_GAME_VIEW_JSON);
    render(<Table />);
    // No decision yet → no staged overlay; the tray carries the global actions.
    expect(screen.queryByTestId('prompt-overlay')).toBeNull();

    // Enter targeting: the anchored overlay stages, carrying the prompt banner.
    fireEvent.click(screen.getByTestId('entity-c3'));
    fireEvent.click(
      within(screen.getByTestId('entity-actions-c3')).getByRole('button', {
        name: 'Cast Lightning Bolt',
      }),
    );
    const overlay = screen.getByTestId('prompt-overlay');
    // The overlay is anchored (a placement was resolved from reported rects) and holds
    // the decision banner, not the tray.
    expect(overlay.getAttribute('data-placement')).toMatch(/above|below/);
    expect(within(overlay).getByTestId('prompt-banner')).toBeDefined();
    expect(within(overlay).getByTestId('targeting-prompt')).toBeDefined();
    // The decision controls (cancel) live in the tray, adjacent to the overlay.
    fireEvent.click(
      within(screen.getByTestId('action-bar')).getByRole('button', { name: 'Cancel targeting' }),
    );
    expect(screen.queryByTestId('prompt-overlay')).toBeNull();
  });

  it('stages the order/zone prompt surface inside the anchored overlay', () => {
    seed(ORDER_GAME_VIEW_JSON);
    render(<Table />);
    fireEvent.click(
      within(screen.getByTestId('action-bar')).getByRole('button', { name: 'Order triggers' }),
    );
    const overlay = screen.getByTestId('prompt-overlay');
    // The reorder list rides the same staged overlay as the banner.
    expect(within(overlay).getByTestId('prompt-surface')).toBeDefined();
  });

  it('renders the deadline countdown within the staged prompt overlay (issue #263)', () => {
    // A targeting view that also carries a server clock: the countdown rides the
    // anchored decision surface, not a detached banner row.
    const withDeadline = JSON.stringify({
      ...JSON.parse(TARGETING_GAME_VIEW_JSON),
      action_deadline: 8,
    });
    seed(withDeadline);
    render(<Table />);
    fireEvent.click(screen.getByTestId('entity-c3'));
    fireEvent.click(
      within(screen.getByTestId('entity-actions-c3')).getByRole('button', {
        name: 'Cast Lightning Bolt',
      }),
    );
    const countdown = within(screen.getByTestId('prompt-overlay')).getByTestId(
      'deadline-countdown',
    );
    // Seeded under the 10s low-time threshold → the warning state is shown.
    expect(countdown.textContent).toContain('8s');
    expect(countdown.getAttribute('data-low')).toBe('true');
  });

  it('shows the priority-window countdown quietly in the tray when no decision is staged', () => {
    // The sample view carries a clock and a bare priority window (no forced decision).
    seed(SAMPLE_GAME_VIEW_JSON);
    render(<Table />);
    expect(screen.queryByTestId('prompt-overlay')).toBeNull();
    const countdown = within(screen.getByTestId('action-bar')).getByTestId('deadline-countdown');
    expect(countdown.textContent).toContain('13s');
  });

  it('reconstructs the staged overlay from a replayed mid-prompt view (rehydration)', () => {
    seed(DECLARE_ATTACKERS_GAME_VIEW_JSON);
    render(<Table />);
    const enter = (): void => {
      fireEvent.click(
        within(screen.getByTestId('action-bar')).getByRole('button', {
          name: 'Declare attackers',
        }),
      );
    };
    enter();
    expect(screen.getByTestId('prompt-overlay')).toBeDefined();
    // Replaying the same view drops the ephemeral session (no state across messages)…
    act(() => useGameStore.getState().ingest(DECLARE_ATTACKERS_GAME_VIEW_JSON));
    expect(screen.queryByTestId('prompt-overlay')).toBeNull();
    // …and re-entering stages the identical overlay again.
    enter();
    expect(screen.getByTestId('prompt-overlay')).toBeDefined();
    expect(
      within(screen.getByTestId('prompt-overlay')).getByTestId('multiselect-prompt').textContent,
    ).toContain('Choose attackers');
  });
});

describe('Table keyboard parity (issue #266)', () => {
  it('toggles the shortcut reference with "?" and closes it with Escape', () => {
    seed(SAMPLE_GAME_VIEW_JSON);
    render(<Table />);
    expect(screen.queryByTestId('shortcut-help')).toBeNull();
    fireEvent.keyDown(window, { key: '?' });
    const help = screen.getByTestId('shortcut-help');
    // Pass is offered in the sample view, so its binding reads as available.
    expect(within(help).getByTestId('shortcut-pass').getAttribute('data-available')).toBe('true');
    fireEvent.keyDown(window, { key: 'Escape' });
    expect(screen.queryByTestId('shortcut-help')).toBeNull();
  });

  it('passes priority with "P" when the action is offered', () => {
    const choose = seed(SAMPLE_GAME_VIEW_JSON);
    render(<Table />);
    fireEvent.keyDown(window, { key: 'p' });
    expect(choose).toHaveBeenCalledTimes(1);
    expect((choose.mock.calls[0][0] as ValidAction).id).toBe('a1');
  });

  it('leaves "P" inert when no pass action exists', () => {
    // A view whose only action is a subject action — no pass on offer.
    const noPass = JSON.stringify({
      you: 'p1',
      my_hand: [],
      opponents: [{ player_id: 'p2', hand_size: 2, life: 20, library_size: 40, graveyard_size: 0 }],
      battlefield: [
        {
          id: 'perm_x',
          controller: 'p1',
          owner: 'p1',
          card: { id: 'perm_x', name: 'Elf', type_line: 'Creature' },
        },
      ],
      phase: 'precombat_main',
      valid_actions: [{ id: 'aX', type: 'activate_ability', label: 'Tap', subject: ['perm_x'] }],
    });
    const choose = seed(noPass);
    render(<Table />);
    fireEvent.keyDown(window, { key: 'p' });
    expect(choose).not.toHaveBeenCalled();
  });

  it('activates the focused control with Enter (reusing its click handler)', () => {
    const choose = seed(SAMPLE_GAME_VIEW_JSON);
    render(<Table />);
    const pass = within(screen.getByTestId('action-bar')).getByRole('button', { name: 'Pass' });
    pass.focus();
    fireEvent.keyDown(window, { key: 'Enter' });
    expect(choose).toHaveBeenCalledTimes(1);
    expect((choose.mock.calls[0][0] as ValidAction).id).toBe('a1');
  });

  it('moves focus between controls with the arrow keys (never trapped)', () => {
    seed(SAMPLE_GAME_VIEW_JSON);
    render(<Table />);
    fireEvent.keyDown(window, { key: 'ArrowRight' });
    const first = document.activeElement;
    expect(first).toBeInstanceOf(HTMLButtonElement);
    fireEvent.keyDown(window, { key: 'ArrowRight' });
    expect(document.activeElement).toBeInstanceOf(HTMLButtonElement);
    expect(document.activeElement).not.toBe(first);
  });

  it('inspects the focused card with "I"', () => {
    seed(SAMPLE_GAME_VIEW_JSON);
    render(<Table />);
    screen.getByTestId('entity-perm_xyz').focus();
    fireEvent.keyDown(window, { key: 'i' });
    expect(screen.getByTestId('card-inspect-name').textContent).toBe('Grizzly Bears');
  });

  it('drives a targeting pick entirely by keyboard', () => {
    const choose = seed(TARGETING_GAME_VIEW_JSON);
    render(<Table />);
    // Open targeting via the entity + its cast action (focus + Enter, no pointer).
    screen.getByTestId('entity-c3').focus();
    fireEvent.keyDown(window, { key: 'Enter' });
    within(screen.getByTestId('entity-actions-c3'))
      .getByRole('button', { name: 'Cast Lightning Bolt' })
      .focus();
    fireEvent.keyDown(window, { key: 'Enter' });
    // Now in targeting: focus a candidate and submit with Enter.
    expect(choose).not.toHaveBeenCalled();
    screen.getByTestId('target-perm_xyz').focus();
    fireEvent.keyDown(window, { key: 'Enter' });
    expect(choose).toHaveBeenCalledTimes(1);
    const [, targets] = choose.mock.calls[0] as [ValidAction, TargetChoice[]];
    expect(targets).toEqual([{ slot: 't0', chosen: ['perm_xyz'] }]);
  });

  it('toggles a multi-select candidate with Space and confirms with Enter', () => {
    const choose = seed(DECLARE_ATTACKERS_GAME_VIEW_JSON);
    render(<Table />);
    within(screen.getByTestId('action-bar'))
      .getByRole('button', { name: 'Declare attackers' })
      .focus();
    fireEvent.keyDown(window, { key: 'Enter' });
    // Space toggles the focused candidate into the selection.
    screen.getByTestId('target-atk_1').focus();
    fireEvent.keyDown(window, { key: ' ' });
    expect(screen.getByTestId('multiselect-count').textContent).toContain('1 selected');
    // Enter with nothing focused confirms the primary (the enabled multi-select).
    (document.activeElement as HTMLElement | null)?.blur();
    fireEvent.keyDown(window, { key: 'Enter' });
    expect(choose).toHaveBeenCalledTimes(1);
    const [action, targets] = choose.mock.calls[0] as [ValidAction, TargetChoice[]];
    expect(action.id).toBe('a5');
    expect(targets).toEqual([{ slot: 'attackers', chosen: ['atk_1'] }]);
  });
});

describe('Table spatial focus model (issue #301)', () => {
  /**
   * Press an arrow `key` (the region-aware *move focus* verb) until the focused
   * element satisfies `match`, proving the target is reachable purely by keyboard
   * navigation — never by a `.focus()` shortcut. Bounded so a miss fails loudly.
   */
  function arrowUntil(key: string, match: (el: Element | null) => boolean, max = 80): void {
    for (let i = 0; i < max; i += 1) {
      fireEvent.keyDown(window, { key });
      if (match(document.activeElement)) return;
    }
    throw new Error(`focus never reached the target after ${max} "${key}" presses`);
  }
  const byTestId =
    (id: string) =>
    (el: Element | null): boolean =>
      el?.getAttribute('data-testid') === id;
  const byName =
    (name: string) =>
    (el: Element | null): boolean =>
      el?.textContent?.trim() === name;

  it('drives a full targeting flow keyboard-only through region navigation', () => {
    const choose = seed(TARGETING_GAME_VIEW_JSON);
    render(<Table />);

    // Reach the spell card's on-entity hotspot by arrow navigation, then select it.
    arrowUntil('ArrowRight', byTestId('entity-c3'));
    fireEvent.keyDown(window, { key: 'Enter' });
    // Its cast action now renders on the entity; reach the chip by arrows and confirm.
    expect(screen.getByTestId('entity-actions-c3')).toBeDefined();
    arrowUntil('ArrowRight', byName('Cast Lightning Bolt'));
    fireEvent.keyDown(window, { key: 'Enter' });
    // Choosing a targeted action opens targeting — nothing is submitted yet.
    expect(choose).not.toHaveBeenCalled();

    // Navigate to a server candidate hotspot and submit the pick with Enter.
    arrowUntil('ArrowRight', byTestId('target-perm_xyz'));
    fireEvent.keyDown(window, { key: 'Enter' });
    expect(choose).toHaveBeenCalledTimes(1);
    const [action, targets] = choose.mock.calls[0] as [ValidAction, TargetChoice[]];
    expect(action.id).toBe('a3');
    expect(targets).toEqual([{ slot: 't0', chosen: ['perm_xyz'] }]);
  });

  it('drives a full multi-select flow keyboard-only through region navigation', () => {
    const choose = seed(DECLARE_ATTACKERS_GAME_VIEW_JSON);
    render(<Table />);

    // Reach the subject-less multi-select action in the tray region and open it.
    arrowUntil('ArrowRight', byName('Declare attackers'));
    fireEvent.keyDown(window, { key: 'Enter' });
    expect(screen.getByTestId('multiselect-prompt')).toBeDefined();

    // Toggle both attacker candidates with Space, reaching each by arrow navigation.
    arrowUntil('ArrowRight', byTestId('target-atk_1'));
    fireEvent.keyDown(window, { key: ' ' });
    expect(screen.getByTestId('multiselect-count').textContent).toContain('1 selected');
    arrowUntil('ArrowRight', byTestId('target-atk_2'));
    fireEvent.keyDown(window, { key: ' ' });
    expect(screen.getByTestId('multiselect-count').textContent).toContain('2 selected');

    // Navigate to the confirm control and commit the whole selection atomically.
    arrowUntil('ArrowRight', byTestId('multiselect-confirm'));
    fireEvent.keyDown(window, { key: 'Enter' });
    expect(choose).toHaveBeenCalledTimes(1);
    const [action, targets] = choose.mock.calls[0] as [ValidAction, TargetChoice[]];
    expect(action.id).toBe('a5');
    expect(targets).toEqual([{ slot: 'attackers', chosen: ['atk_1', 'atk_2'] }]);
  });

  it('reaches a rail (stack) control by keyboard, proving cross-region navigation', () => {
    // The stack/activity rail is a vertical (column) region: cross-region arrows land
    // in it, and its own axis (Up/Down) walks its items — so its stack inspect handle
    // is reachable by keyboard like every other surface (canvas hotspots included).
    seed(SAMPLE_GAME_VIEW_JSON);
    render(<Table />);
    // Cross into the rail from the board with Right, then walk its items with Down.
    arrowUntil('ArrowRight', byTestId('rail-collapse'));
    arrowUntil('ArrowDown', byTestId('inspect-s1'));
    expect(document.activeElement?.getAttribute('data-testid')).toBe('inspect-s1');
  });
});

describe('Table multiplayer table (3–4 players, issue #348)', () => {
  /** The nearest ancestor focus region's id, or null — the region the focus engine
   * (issue #301) walks this element as part of. */
  function focusRegionOf(el: Element): string | null {
    return el.closest('[data-focus-region]')?.getAttribute('data-focus-region') ?? null;
  }
  /** Whether the focus engine treats this element as a reachable item: a live button
   * or an explicit `data-focus-item`, sitting inside a focus region. */
  function isFocusReachable(el: Element): boolean {
    const focusable =
      (el.tagName === 'BUTTON' && !el.hasAttribute('disabled')) ||
      el.hasAttribute('data-focus-item');
    return focusable && focusRegionOf(el) !== null;
  }

  it('renders a HUD tile, board area, and zone piles for every opponent', () => {
    seed(FOUR_PLAYER_GAME_VIEW_JSON);
    render(<Table />);
    // A HUD tile for each of the three opponents (the receiver lives in the dock).
    for (const id of ['p2', 'p3', 'p4']) {
      expect(screen.getByTestId(`tile-${id}`)).toBeDefined();
    }
    // The eliminated seat's tile announces its state to assistive tech.
    expect(screen.getByTestId('tile-p3').getAttribute('aria-label')).toContain('eliminated');
    // Each opponent's board permanents render as inspectable surfaces…
    for (const id of ['p2_blk', 'p2_land', 'p4_crt', 'p4_land']) {
      expect(screen.getByTestId(`inspect-surface-${id}`)).toBeDefined();
    }
    // …and every seat has its own graveyard pile on the board (count lives here).
    for (const id of ['p1', 'p2', 'p3', 'p4']) {
      expect(screen.getByTestId(`table-graveyard-${id}`)).toBeDefined();
    }
  });

  it('makes every opponent area — board, piles, and HUD tile — keyboard-reachable', () => {
    seed(FOUR_PLAYER_GAME_VIEW_JSON);
    render(<Table />);
    // Each opponent's HUD tile is a focus item in the opponent-HUD region, so
    // keyboard/controller focus can land on the tile itself — not just the board.
    for (const id of ['p2', 'p3', 'p4']) {
      const tile = screen.getByTestId(`tile-${id}`);
      expect(isFocusReachable(tile)).toBe(true);
      expect(focusRegionOf(tile)).toBe('opponentHud');
      expect(tile.getAttribute('tabindex')).toBe('0');
    }
    // Each opponent's board permanents are reachable inspect surfaces in the battlefield.
    for (const id of ['p2_blk', 'p4_crt']) {
      const surface = screen.getByTestId(`inspect-surface-${id}`);
      expect(isFocusReachable(surface)).toBe(true);
      expect(focusRegionOf(surface)).toBe('battlefield');
    }
    // Each opponent's graveyard pile is a reachable button in the battlefield.
    for (const id of ['p2', 'p3', 'p4']) {
      const pile = screen.getByTestId(`table-graveyard-${id}`);
      expect(isFocusReachable(pile)).toBe(true);
      expect(focusRegionOf(pile)).toBe('battlefield');
    }
  });

  it('keeps the two-player opponent tile as quiet display (no focus stop)', () => {
    // The duel is untouched: a single opponent's tile is not a focus anchor, so the
    // finely-tuned two-player focus order does not change (issue #348 AC: 2p unchanged).
    seed(SAMPLE_GAME_VIEW_JSON);
    render(<Table />);
    const tile = screen.getByTestId('tile-p2');
    expect(tile.hasAttribute('data-focus-item')).toBe(false);
    expect(tile.hasAttribute('tabindex')).toBe(false);
  });
});

describe('Table zone browsers (issue #262)', () => {
  it('opens the local graveyard from the board pile and lists it in order', () => {
    seed(ZONES_GAME_VIEW_JSON);
    render(<Table />);
    expect(screen.queryByTestId('zone-browser')).toBeNull();
    fireEvent.click(screen.getByTestId('table-graveyard-p1'));
    const browser = screen.getByTestId('zone-browser');
    expect(within(browser).getByTestId('zone-browser-title').textContent).toContain(
      'p1 — Graveyard',
    );
    expect(within(browser).getByTestId('browser-card-gy_p1_a')).toBeDefined();
    expect(within(browser).getByTestId('browser-card-gy_p1_b')).toBeDefined();
  });

  it("opens an opponent's graveyard (public zone) from their board pile", () => {
    seed(ZONES_GAME_VIEW_JSON);
    render(<Table />);
    fireEvent.click(screen.getByTestId('table-graveyard-p2'));
    expect(screen.getByTestId('browser-card-gy_p2_a').textContent).toContain('Lightning Bolt');
  });

  it('opens the exile browser and inspects a card inside it', () => {
    seed(ZONES_GAME_VIEW_JSON);
    render(<Table />);
    fireEvent.click(screen.getByTestId('table-exile-p1'));
    expect(screen.getByTestId('zone-browser-title').textContent).toContain('p1 — Exile');
    // A card inside the browser opens the shared inspect popover (issue #261 reuse).
    fireEvent.click(screen.getByTestId('browser-card-ex_p1_a'));
    expect(screen.getByTestId('card-inspect-name').textContent).toBe('Forest');
    // Closing inspect leaves the browser open beneath it.
    fireEvent.click(screen.getByTestId('card-inspect-close'));
    expect(screen.queryByTestId('card-inspect')).toBeNull();
    expect(screen.getByTestId('zone-browser')).toBeDefined();
  });

  it('shows the empty-zone state for a player with no exile, and closes on Escape', () => {
    seed(ZONES_GAME_VIEW_JSON);
    render(<Table />);
    // p2 has no exile pile in the view → browses empty.
    fireEvent.click(screen.getByTestId('table-exile-p2'));
    expect(screen.getByTestId('zone-browser-empty')).toBeDefined();
    fireEvent.keyDown(window, { key: 'Escape' });
    expect(screen.queryByTestId('zone-browser')).toBeNull();
  });

  it('drops an open browser when a fresh GameView arrives (no state across messages)', () => {
    seed(ZONES_GAME_VIEW_JSON);
    render(<Table />);
    fireEvent.click(screen.getByTestId('table-graveyard-p1'));
    expect(screen.getByTestId('zone-browser')).toBeDefined();
    act(() => useGameStore.getState().ingest(SAMPLE_GAME_VIEW_JSON));
    expect(screen.queryByTestId('zone-browser')).toBeNull();
  });

  it('browses zones in the read-only game-over state', () => {
    const terminal = JSON.stringify({
      you: 'p1',
      my_hand: [],
      opponents: [{ player_id: 'p2', hand_size: 0, life: 0, library_size: 40, graveyard_size: 0 }],
      battlefield: [],
      graveyards: [
        { player_id: 'p1', cards: [{ id: 'gy_end', name: 'Shock', type_line: 'Instant' }] },
      ],
      phase: 'end',
      valid_actions: [],
      result: { winner: 'p1', losers: ['p2'], reason: 'life_zero' },
    });
    seed(terminal);
    render(<Table />);
    fireEvent.click(screen.getByTestId('table-graveyard-p1'));
    expect(screen.getByTestId('browser-card-gy_end').textContent).toContain('Shock');
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

describe('Table game log (issue #260)', () => {
  it('renders the game log in the rail with client-composed prose', () => {
    seed(SAMPLE_GAME_VIEW_JSON);
    render(<Table />);
    const log = screen.getByTestId('game-log');
    // A spell-cast entry composed client-side from the structured event.
    expect(within(log).getByTestId('log-entry-35').textContent).toBe('p2 cast Lightning Bolt.');
    // The leading run of consecutive step changes collapses behind one summary.
    expect(within(log).getByTestId('log-steps')).toBeDefined();
  });

  it('highlights a referenced player tile on click, and toggles it off', () => {
    seed(SAMPLE_GAME_VIEW_JSON);
    render(<Table />);
    // The cast entry references p2; before any click the tile is not highlighted.
    expect(screen.getByTestId('tile-p2').getAttribute('data-highlighted')).toBeNull();
    fireEvent.click(within(screen.getByTestId('game-log')).getByTestId('log-ref-p2'));
    expect(screen.getByTestId('tile-p2').getAttribute('data-highlighted')).toBe('true');
    // Clicking the same reference again clears the highlight (ephemeral, presentational).
    fireEvent.click(within(screen.getByTestId('game-log')).getByTestId('log-ref-p2'));
    expect(screen.getByTestId('tile-p2').getAttribute('data-highlighted')).toBeNull();
  });

  it('highlighting a log reference opens no action tray (purely presentational)', () => {
    const choose = seed(SAMPLE_GAME_VIEW_JSON);
    render(<Table />);
    fireEvent.click(within(screen.getByTestId('game-log')).getByTestId('log-ref-perm_xyz'));
    // No selection echo / entity actions surface — highlighting derives nothing.
    expect(screen.queryByTestId('entity-actions-perm_xyz')).toBeNull();
    expect(choose).not.toHaveBeenCalled();
  });

  it('keeps the log visible and interactive in the read-only game-over state', () => {
    // A terminal frame that still carries a history window: the log rides the rail in
    // the game-over branch too, and its references still highlight (issue #260 scope).
    const gameOverWithLog = JSON.stringify({
      you: 'p1',
      opponents: [{ player_id: 'p2', hand_size: 0, life: 0, library_size: 40 }],
      phase: 'end',
      valid_actions: [],
      result: { winner: 'p1', losers: ['p2'], reason: 'life_zero' },
      log: [
        {
          sequence: 1,
          event: { type: 'spell_cast', player: 'p2', card: { id: 's9', name: 'Shock' } },
        },
        {
          sequence: 2,
          event: {
            type: 'game_over',
            result: { winner: 'p1', losers: ['p2'], reason: 'life_zero' },
          },
        },
      ],
    });
    seed(gameOverWithLog);
    render(<Table />);
    expect(screen.getByTestId('table-game-over')).toBeDefined();
    const log = screen.getByTestId('game-log');
    expect(within(log).getByTestId('log-entry-2').textContent).toBe(
      'Game over — p1 wins (life total reached zero).',
    );
    // A reference still highlights the player's tile in the read-only terminal state.
    fireEvent.click(within(log).getByTestId('log-ref-p2'));
    expect(screen.getByTestId('tile-p2').getAttribute('data-highlighted')).toBe('true');
  });
});
