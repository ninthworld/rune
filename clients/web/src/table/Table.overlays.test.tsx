import { describe, expect, it } from 'vitest';
import { act, fireEvent, render, screen, within } from '@testing-library/react';
import {
  GAME_OVER_DRAW_JSON,
  GAME_OVER_LOSS_JSON,
  GAME_OVER_WIN_JSON,
  SAMPLE_GAME_VIEW_JSON,
  TARGETING_GAME_VIEW_JSON,
  ZONES_GAME_VIEW_JSON,
} from '../game-view.fixture';
import { useGameStore } from '../store';
import { Table } from './Table';
import { registerTableTestHooks, seed } from './table-test-support';

registerTableTestHooks();

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
      within(screen.getByTestId('selection-echo')).getByRole('button', {
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
