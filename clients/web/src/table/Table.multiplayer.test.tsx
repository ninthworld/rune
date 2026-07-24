import { beforeEach, describe, expect, it, vi } from 'vitest';
import { act, fireEvent, render, screen, within } from '@testing-library/react';
import {
  DECLARE_ATTACKERS_MULTIPLAYER_GAME_VIEW_JSON,
  FOUR_PLAYER_GAME_VIEW_JSON,
  SAMPLE_GAME_VIEW_JSON,
} from '../game-view.fixture';
import type { TargetChoice, ValidAction } from '../protocol';
import { useGameStore } from '../store';
import { Table } from './Table';
import { registerTableTestHooks, seed } from './table-test-support';

registerTableTestHooks();

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
    // Each opponent's HUD tile is a focus item in their carved panel (the canvas
    // region), so keyboard/controller focus can land on the tile itself — not just
    // the board.
    for (const id of ['p2', 'p3', 'p4']) {
      const tile = screen.getByTestId(`tile-${id}`);
      expect(isFocusReachable(tile)).toBe(true);
      expect(focusRegionOf(tile)).toBe('canvas');
      expect(tile.getAttribute('tabindex')).toBe('0');
    }
    // Each opponent's board permanents are reachable inspect surfaces on the canvas.
    for (const id of ['p2_blk', 'p4_crt']) {
      const surface = screen.getByTestId(`inspect-surface-${id}`);
      expect(isFocusReachable(surface)).toBe(true);
      expect(focusRegionOf(surface)).toBe('canvas');
    }
    // Each opponent's graveyard pile is a reachable button in their panel chrome.
    for (const id of ['p2', 'p3', 'p4']) {
      const pile = screen.getByTestId(`table-graveyard-${id}`);
      expect(isFocusReachable(pile)).toBe(true);
      expect(focusRegionOf(pile)).toBe('canvas');
    }
  });

  it('points the attack treatment at each attacked player’s HUD tile (issue #347)', () => {
    seed(FOUR_PLAYER_GAME_VIEW_JSON);
    render(<Table />);
    // p1's split attack hits p2 and p4 (one attacker each); p3 is not attacked.
    expect(screen.getByTestId('hud-attacked-p2').textContent).toContain('×1');
    expect(screen.getByTestId('hud-attacked-p4').textContent).toContain('×1');
    expect(screen.queryByTestId('hud-attacked-p3')).toBeNull();
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

describe('Table multi-select: multiplayer declare attackers (issue #347)', () => {
  let choose: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    choose = seed(DECLARE_ATTACKERS_MULTIPLAYER_GAME_VIEW_JSON);
    render(<Table />);
  });

  function enter(): void {
    fireEvent.click(
      within(screen.getByTestId('action-bar')).getByRole('button', { name: 'Declare attackers' }),
    );
  }

  it('assigns two attackers to different defenders and submits atomically (pointer/touch)', () => {
    enter();
    // Pick both attackers on the board.
    fireEvent.click(screen.getByTestId('target-perm_1'));
    fireEvent.click(screen.getByTestId('target-perm_2'));
    // Advance to the first attacker's defender pick; the prompt names the attacker.
    fireEvent.click(within(screen.getByTestId('action-bar')).getByRole('button', { name: 'Next' }));
    expect(screen.getByTestId('multiselect-prompt').textContent).toContain('Charging Rhino');
    // The defenders are chosen from their HUD tiles (players, not board cards).
    expect(screen.queryByTestId('target-perm_1')).toBeNull();
    fireEvent.click(screen.getByTestId('target-player-p3')); // Rhino attacks p3
    // Picking a defender auto-advances to the next attacker's target choice.
    expect(screen.getByTestId('multiselect-prompt').textContent).toContain('Skyshroud Falcon');
    fireEvent.click(screen.getByTestId('target-player-p2')); // Falcon attacks p2
    // Confirm the whole split declaration in one atomic answer.
    fireEvent.click(screen.getByTestId('multiselect-confirm'));
    expect(choose).toHaveBeenCalledTimes(1);
    const [action, targets] = choose.mock.calls[0] as [ValidAction, TargetChoice[]];
    expect(action.id).toBe('a5');
    expect(action.token).toBe('h:atk0');
    expect(targets).toEqual([
      { slot: 'attackers', chosen: ['perm_1', 'perm_2'] },
      { slot: 'defend_1', chosen: ['p3'] },
      { slot: 'defend_2', chosen: ['p2'] },
    ]);
  });

  it('only asks for a defender for the attackers actually declared', () => {
    enter();
    // Declare a single attacker; there must be exactly one defender step (its own).
    fireEvent.click(screen.getByTestId('target-perm_1'));
    fireEvent.click(within(screen.getByTestId('action-bar')).getByRole('button', { name: 'Next' }));
    expect(screen.getByTestId('multiselect-prompt').textContent).toContain('Charging Rhino');
    fireEvent.click(screen.getByTestId('target-player-p2'));
    // No further defender step for the undeclared falcon — confirm submits now.
    fireEvent.click(screen.getByTestId('multiselect-confirm'));
    const [, targets] = choose.mock.calls[0] as [ValidAction, TargetChoice[]];
    expect(targets).toEqual([
      { slot: 'attackers', chosen: ['perm_1'] },
      { slot: 'defend_1', chosen: ['p2'] },
    ]);
  });

  it('keeps the empty declaration a one-step, defender-free flow', () => {
    enter();
    // No attackers → no defender step → immediately confirmable, unchanged from 2p.
    fireEvent.click(screen.getByTestId('multiselect-confirm'));
    const [, targets] = choose.mock.calls[0] as [ValidAction, TargetChoice[]];
    expect(targets).toEqual([{ slot: 'attackers', chosen: [] }]);
  });

  it('completes the whole declaration by keyboard (Enter/Space activate)', () => {
    // Open via Enter on the focused action; select-then-confirm reuses click handlers,
    // so keyboard drives the same flow (issue #347: keyboard path completes).
    within(screen.getByTestId('action-bar'))
      .getByRole('button', { name: 'Declare attackers' })
      .focus();
    fireEvent.keyDown(window, { key: 'Enter' });
    screen.getByTestId('target-perm_1').focus();
    fireEvent.keyDown(window, { key: ' ' });
    // Advance to the defender pick, then choose the defending player by keyboard.
    within(screen.getByTestId('action-bar')).getByRole('button', { name: 'Next' }).focus();
    fireEvent.keyDown(window, { key: 'Enter' });
    screen.getByTestId('target-player-p3').focus();
    fireEvent.keyDown(window, { key: 'Enter' });
    screen.getByTestId('multiselect-confirm').focus();
    fireEvent.keyDown(window, { key: 'Enter' });
    expect(choose).toHaveBeenCalledTimes(1);
    const [, targets] = choose.mock.calls[0] as [ValidAction, TargetChoice[]];
    expect(targets).toEqual([
      { slot: 'attackers', chosen: ['perm_1'] },
      { slot: 'defend_1', chosen: ['p3'] },
    ]);
  });

  it('drops the in-progress split declaration when a fresh view arrives', () => {
    enter();
    fireEvent.click(screen.getByTestId('target-perm_1'));
    act(() => useGameStore.getState().ingest(SAMPLE_GAME_VIEW_JSON));
    expect(screen.queryByTestId('multiselect-prompt')).toBeNull();
    expect(choose).not.toHaveBeenCalled();
  });
});
