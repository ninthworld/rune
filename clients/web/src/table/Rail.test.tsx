import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import type { GameView, StackItem } from '../protocol';
import type { Rect } from './scene';
import { Rail } from './Rail';

afterEach(cleanup);

/** A minimal live {@link GameView} carrying just the fields the rail/stack read. */
function viewWith(stack: StackItem[]): GameView {
  return {
    you: 'p1',
    my_hand: [],
    me: { life: 20, library_size: 40 },
    opponents: [],
    battlefield: [],
    stack,
    graveyards: [],
    exile: [],
    phase: 'precombat_main',
    turn: 1,
    active_player: 'p1',
    mana_pool: [],
    valid_actions: [],
    player_names: {},
  };
}

/** The docked rail column rect (wide geometry). */
const DOCKED: Rect = { x: 1024, y: 124, w: 256, h: 676 };
/** The 44px badge anchor rect (narrow geometry). */
const BADGE_ANCHOR: Rect = { x: 656, y: 132, w: 44, h: 44 };

const oneSpell: StackItem[] = [{ id: 's1', controller: 'p1', description: 'Lightning Bolt' }];
const twoSpells: StackItem[] = [
  { id: 's1', controller: 'p1', description: 'Grizzly Bears' },
  { id: 's2', controller: 'p2', description: 'Counterspell' },
];

describe('Rail (issue #299)', () => {
  it('renders nothing for an empty stack (claims no meaningful width)', () => {
    const { container } = render(<Rail view={viewWith([])} rect={DOCKED} collapsed={false} />);
    expect(container.firstChild).toBeNull();
    expect(screen.queryByTestId('rail')).toBeNull();
    expect(screen.queryByTestId('rail-badge')).toBeNull();
  });

  it('auto-expands the panel when the stack is populated on wide geometry', () => {
    render(<Rail view={viewWith(oneSpell)} rect={DOCKED} collapsed={false} />);
    // The expanded panel (stack section + reserved log slot) shows by default; no badge.
    expect(screen.getByTestId('rail')).toBeDefined();
    expect(screen.getByTestId('stack-panel')).toBeDefined();
    expect(screen.getByTestId('rail-log-slot')).toBeDefined();
    expect(screen.queryByTestId('rail-badge')).toBeNull();
    expect(screen.getByTestId('stack-item-s1').textContent).toContain('Lightning Bolt');
  });

  it('collapses to a count badge by default on narrow geometry', () => {
    render(<Rail view={viewWith(twoSpells)} rect={BADGE_ANCHOR} collapsed={true} />);
    const badge = screen.getByTestId('rail-badge');
    // The badge shows the live object count and is a real button (pointer + keyboard
    // + touch reachable) — the panel and its stack section are hidden until expanded.
    expect(badge.tagName).toBe('BUTTON');
    expect(badge.textContent).toContain('2');
    expect(badge.getAttribute('aria-expanded')).toBe('false');
    expect(screen.queryByTestId('rail')).toBeNull();
    expect(screen.queryByTestId('stack-panel')).toBeNull();
  });

  it('reflects the live object count in the badge', () => {
    const { rerender } = render(
      <Rail view={viewWith(oneSpell)} rect={BADGE_ANCHOR} collapsed={true} />,
    );
    expect(screen.getByTestId('rail-badge').textContent).toContain('1');
    rerender(<Rail view={viewWith(twoSpells)} rect={BADGE_ANCHOR} collapsed={true} />);
    expect(screen.getByTestId('rail-badge').textContent).toContain('2');
  });

  it('expands from the badge on click (pointer/touch), then collapses back', () => {
    render(<Rail view={viewWith(oneSpell)} rect={BADGE_ANCHOR} collapsed={true} />);
    fireEvent.click(screen.getByTestId('rail-badge'));
    // The expanded panel now floats over the board; the stack is inspectable/pickable.
    expect(screen.getByTestId('rail')).toBeDefined();
    expect(screen.getByTestId('stack-panel')).toBeDefined();
    expect(screen.queryByTestId('rail-badge')).toBeNull();
    // The collapse control returns it to the badge (manual, ephemeral).
    fireEvent.click(screen.getByTestId('rail-collapse'));
    expect(screen.getByTestId('rail-badge')).toBeDefined();
    expect(screen.queryByTestId('rail')).toBeNull();
  });

  it('collapses the docked panel to a badge on manual collapse (wide geometry)', () => {
    render(<Rail view={viewWith(oneSpell)} rect={DOCKED} collapsed={false} />);
    expect(screen.getByTestId('rail')).toBeDefined();
    fireEvent.click(screen.getByTestId('rail-collapse'));
    expect(screen.getByTestId('rail-badge')).toBeDefined();
    expect(screen.queryByTestId('rail')).toBeNull();
  });

  it('resolves the default again on a fresh view (manual state is ephemeral)', () => {
    const { rerender } = render(<Rail view={viewWith(oneSpell)} rect={DOCKED} collapsed={false} />);
    // Manually collapse the docked rail...
    fireEvent.click(screen.getByTestId('rail-collapse'));
    expect(screen.getByTestId('rail-badge')).toBeDefined();
    // ...a fresh view (new object identity) discards the override and re-expands to
    // the wide default — the rail is reconstructable from one GameView + geometry.
    rerender(<Rail view={viewWith(twoSpells)} rect={DOCKED} collapsed={false} />);
    expect(screen.getByTestId('rail')).toBeDefined();
    expect(screen.queryByTestId('rail-badge')).toBeNull();
  });

  it('keeps stack objects pickable in targeting mode inside the rail', () => {
    const onPick = vi.fn();
    render(
      <Rail
        view={viewWith(twoSpells)}
        rect={DOCKED}
        collapsed={false}
        targeting={{ candidates: ['s1'], onPick }}
      />,
    );
    fireEvent.click(screen.getByTestId('target-s1'));
    expect(onPick).toHaveBeenCalledWith('s1');
  });

  it('keeps stack objects inspectable in the rail', () => {
    const onInspect = vi.fn();
    render(
      <Rail view={viewWith(twoSpells)} rect={DOCKED} collapsed={false} onInspect={onInspect} />,
    );
    fireEvent.click(screen.getByTestId('inspect-s2'));
    expect(onInspect).toHaveBeenCalledWith('s2');
  });
});
