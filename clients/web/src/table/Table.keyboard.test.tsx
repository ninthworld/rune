import { describe, expect, it } from 'vitest';
import { fireEvent, render, screen, within } from '@testing-library/react';
import {
  DECLARE_ATTACKERS_GAME_VIEW_JSON,
  SAMPLE_GAME_VIEW_JSON,
  TARGETING_GAME_VIEW_JSON,
} from '../game-view.fixture';
import type { TargetChoice, ValidAction } from '../protocol';
import { Table } from './Table';
import { registerTableTestHooks, seed } from './table-test-support';

registerTableTestHooks();

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
    // Open targeting via the entity + its dock-routed cast action (focus + Enter,
    // no pointer — the selection routes its actions to the one action home).
    screen.getByTestId('entity-c3').focus();
    fireEvent.keyDown(window, { key: 'Enter' });
    within(screen.getByTestId('selection-echo'))
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
    // Its cast action now routes to the dock; cross into the dock region with
    // Right, then walk the (column) dock's items with Down to reach the button.
    expect(screen.getByTestId('selection-echo')).toBeDefined();
    arrowUntil(
      'ArrowRight',
      (el) => el !== null && el.closest('[data-focus-region="dock"]') !== null,
    );
    arrowUntil('ArrowDown', byName('Cast Lightning Bolt'));
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
    arrowUntil(
      'ArrowRight',
      (el) => el?.closest('[data-focus-region="rail"]') !== null && el !== null,
    );
    if (document.activeElement?.getAttribute('data-testid') !== 'inspect-s1') {
      arrowUntil('ArrowDown', byTestId('inspect-s1'));
    }
    expect(document.activeElement?.getAttribute('data-testid')).toBe('inspect-s1');
  });
});
