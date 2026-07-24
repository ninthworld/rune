import { beforeEach, describe, expect, it, vi } from 'vitest';
import { act, fireEvent, render, screen, within } from '@testing-library/react';
import { SAMPLE_GAME_VIEW_JSON } from '../game-view.fixture';
import type { ValidAction } from '../protocol';
import { useGameStore } from '../store';
import { Table } from './Table';
import { registerTableTestHooks, seed } from './table-test-support';

registerTableTestHooks();

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

  it('fires an entity action via the dock (select-then-act, one action home)', () => {
    // Select the permanent via its on-entity hotspot...
    fireEvent.click(screen.getByTestId('entity-perm_xyz'));
    // ...its actions route to the dock (ADR 0023: per-card popups are abolished).
    expect(screen.queryByTestId('entity-actions-perm_xyz')).toBeNull();
    const echo = screen.getByTestId('selection-echo');
    fireEvent.click(within(echo).getByRole('button', { name: 'Tap for mana' }));

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

  it('gives the selection’s actions the primary weight and demotes Pass while selected', () => {
    const bar = screen.getByTestId('action-bar');
    // Neutral dock: Pass is the primary affordance (the button pressed most).
    expect(within(bar).getByRole('button', { name: /Pass/ }).getAttribute('data-primary')).toBe(
      'true',
    );

    // Selecting a card flips the hierarchy: the routed action is the declared
    // intent, so IT carries the primary treatment and renders above the demoted
    // Pass — the brightest control is never one slip away from the wrong verb.
    fireEvent.click(screen.getByTestId('entity-perm_xyz'));
    const cast = within(bar).getByRole('button', { name: 'Tap for mana' });
    const pass = within(bar).getByRole('button', { name: /Pass/ });
    expect(cast.getAttribute('data-primary')).toBe('true');
    expect(pass.getAttribute('data-primary')).toBeNull();
    expect(cast.compareDocumentPosition(pass) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();

    // Clearing the selection restores Pass as the primary.
    fireEvent.click(screen.getByTestId('clear-selection'));
    expect(within(bar).getByRole('button', { name: /Pass/ }).getAttribute('data-primary')).toBe(
      'true',
    );
  });
});

describe('Table reconstructs from one GameView (reconnect/replay)', () => {
  it('rebuilds the whole UI from a replacement frame with no residue', () => {
    seed(SAMPLE_GAME_VIEW_JSON);
    render(<Table />);

    // First frame: opponent p2 at 20 life (HUD strip), our Grizzly Bears interactive.
    expect(screen.getByTestId('hud-life-p2').textContent).toBe('20');
    expect(screen.getByTestId('entity-perm_xyz')).toBeDefined();

    // Our own identity panel shows our own life (issue #255/#296) — a player can read
    // their own life, not only their opponents'. The library count lives ONCE, on the
    // identity panel's card-shaped zone pile (issue #319, ADR 0023: the receiver's
    // piles park in the bottom shell); the action dock never repeats it.
    expect(screen.getByTestId('hud-life-p1').textContent).toBe('18');
    const libraryPile = within(screen.getByTestId('me-piles')).getByTestId('library-pile-p1');
    expect(libraryPile.getAttribute('aria-label')).toBe('p1 library (52)');
    expect(within(libraryPile).getByText('52')).toBeDefined();
    expect(within(screen.getByTestId('action-bar')).queryByText(/Library/)).toBeNull();

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
    // gated (no valid_actions): the dock reads "waiting" quietly (issue #298) and the
    // fixed prompt strip carries no staged decision — only the muted waiting state.
    expect(screen.getByTestId('hud-life-p2').textContent).toBe('7');
    expect(screen.queryByTestId('entity-perm_xyz')).toBeNull();
    expect(
      within(screen.getByTestId('action-bar')).getByTestId('tray-waiting').textContent,
    ).toContain('Waiting');
    expect(screen.queryByTestId('targeting-prompt')).toBeNull();
    expect(screen.queryByTestId('multiselect-prompt')).toBeNull();
    expect(screen.getByTestId('prompt-banner').textContent).toContain('Waiting');
  });
});
