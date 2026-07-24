import { describe, expect, it } from 'vitest';
import { act, fireEvent, render, screen, within } from '@testing-library/react';
import {
  DECLARE_ATTACKERS_GAME_VIEW_JSON,
  GAME_OVER_WIN_JSON,
  ORDER_GAME_VIEW_JSON,
  SAMPLE_GAME_VIEW_JSON,
  TARGETING_GAME_VIEW_JSON,
} from '../game-view.fixture';
import { useGameStore } from '../store';
import { Table } from './Table';
import { registerTableTestHooks, seed } from './table-test-support';

registerTableTestHooks();

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
      within(screen.getByTestId('selection-echo')).getByRole('button', {
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

describe('Table decision staging (issue #298, reinterpreted by ADR 0023)', () => {
  it('dedicates the fixed prompt strip to a staged decision and releases it on cancel', () => {
    seed(TARGETING_GAME_VIEW_JSON);
    render(<Table />);
    // No decision yet → the strip is in its neutral state; no floating overlay
    // exists in the fixed shell (the strip is a permanent region).
    expect(screen.queryByTestId('targeting-prompt')).toBeNull();
    expect(screen.getByTestId('prompt-banner')).toBeDefined();

    // Enter targeting: the strip states the pending question in words.
    fireEvent.click(screen.getByTestId('entity-c3'));
    fireEvent.click(
      within(screen.getByTestId('selection-echo')).getByRole('button', {
        name: 'Cast Lightning Bolt',
      }),
    );
    const banner = screen.getByTestId('prompt-banner');
    expect(within(banner).getByTestId('targeting-prompt')).toBeDefined();
    // The decision controls (cancel) live in the dock — the one action home.
    fireEvent.click(
      within(screen.getByTestId('action-bar')).getByRole('button', { name: 'Cancel targeting' }),
    );
    expect(screen.queryByTestId('targeting-prompt')).toBeNull();
    // The strip itself never disappears — it returns to the neutral state.
    expect(screen.getByTestId('prompt-banner')).toBeDefined();
  });

  it('stages the order/zone prompt surface inside the decision sheet', () => {
    seed(ORDER_GAME_VIEW_JSON);
    render(<Table />);
    fireEvent.click(
      within(screen.getByTestId('action-bar')).getByRole('button', { name: 'Order triggers' }),
    );
    const sheet = screen.getByTestId('decision-sheet');
    // The reorder list rides the viewport-clamped decision sheet.
    expect(within(sheet).getByTestId('prompt-surface')).toBeDefined();
  });

  it('renders the deadline countdown within the staged prompt strip (issue #263)', () => {
    // A targeting view that also carries a server clock: the countdown rides the
    // decision's prompt strip, not a detached banner row.
    const withDeadline = JSON.stringify({
      ...JSON.parse(TARGETING_GAME_VIEW_JSON),
      action_deadline: 8,
    });
    seed(withDeadline);
    render(<Table />);
    fireEvent.click(screen.getByTestId('entity-c3'));
    fireEvent.click(
      within(screen.getByTestId('selection-echo')).getByRole('button', {
        name: 'Cast Lightning Bolt',
      }),
    );
    const countdown = within(screen.getByTestId('prompt-banner')).getByTestId('deadline-countdown');
    // Seeded under the 10s low-time threshold → the warning state is shown.
    expect(countdown.textContent).toContain('8s');
    expect(countdown.getAttribute('data-low')).toBe('true');
  });

  it('shows the priority-window countdown quietly in the tray when no decision is staged', () => {
    // The sample view carries a clock and a bare priority window (no forced decision).
    seed(SAMPLE_GAME_VIEW_JSON);
    render(<Table />);
    expect(screen.queryByTestId('targeting-prompt')).toBeNull();
    const countdown = within(screen.getByTestId('action-bar')).getByTestId('deadline-countdown');
    expect(countdown.textContent).toContain('13s');
  });

  it('reconstructs the staged decision from a replayed mid-prompt view (rehydration)', () => {
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
    expect(screen.getByTestId('multiselect-prompt')).toBeDefined();
    // Replaying the same view drops the ephemeral session (no state across messages)…
    act(() => useGameStore.getState().ingest(DECLARE_ATTACKERS_GAME_VIEW_JSON));
    expect(screen.queryByTestId('multiselect-prompt')).toBeNull();
    // …and re-entering stages the identical decision strip again.
    enter();
    expect(
      within(screen.getByTestId('prompt-banner')).getByTestId('multiselect-prompt').textContent,
    ).toContain('Choose attackers');
  });
});
