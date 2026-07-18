import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import type { GameLogEntry, GameView, StackItem } from '../protocol';
import { Rail } from './Rail';

afterEach(cleanup);

/** A minimal live {@link GameView} carrying just the fields the rail/stack/log read. */
function viewWith(stack: StackItem[], log: GameLogEntry[] = []): GameView {
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
    seat_order: [],
    mana_pool: [],
    valid_actions: [],
    log,
    player_names: {},
  };
}

/** A one-entry log window (a spell cast) for exercising the rail's log slot. */
const oneLog: GameLogEntry[] = [
  { sequence: 1, event: { type: 'spell_cast', player: 'p1', card: { id: 's1', name: 'Bolt' } } },
];

const oneSpell: StackItem[] = [{ id: 's1', controller: 'p1', description: 'Lightning Bolt' }];
const twoSpells: StackItem[] = [
  { id: 's1', controller: 'p1', description: 'Grizzly Bears' },
  { id: 's2', controller: 'p2', description: 'Counterspell' },
];

describe('Rail (ADR 0023, issue #299)', () => {
  it('always renders the carved column with both sections, even when fully quiet', () => {
    render(<Rail view={viewWith([])} />);
    // The rail is fixed chrome: it never collapses to a badge or vanishes, even
    // with nothing on the stack and no log yet.
    expect(screen.getByTestId('rail').getAttribute('data-expanded')).toBe('true');
    expect(screen.getByTestId('rail-stack')).toBeDefined();
    expect(screen.getByTestId('rail-activity')).toBeDefined();
    // The activity section keeps its chrome too, showing the log's own quiet state.
    expect(screen.getByTestId('game-log')).toBeDefined();
    expect(screen.getByTestId('game-log-empty')).toBeDefined();
  });

  it('shows the designed quiet state in the stack section when the stack is empty', () => {
    render(<Rail view={viewWith([], oneLog)} />);
    // No stack panel, but the chrome does not disappear: the quiet state holds the
    // stack's fixed home while the activity section stays populated.
    expect(screen.queryByTestId('stack-panel')).toBeNull();
    expect(screen.getByTestId('stack-quiet').textContent).toContain('Empty');
    expect(screen.getByTestId('log-entry-1').textContent).toBe('p1 cast Bolt.');
  });

  it('renders the stack panel in place of the quiet state when populated', () => {
    render(<Rail view={viewWith(oneSpell)} />);
    expect(screen.getByTestId('stack-panel')).toBeDefined();
    expect(screen.queryByTestId('stack-quiet')).toBeNull();
    expect(screen.getByTestId('stack-item-s1').textContent).toContain('Lightning Bolt');
    // The activity section is present alongside — both sections, always.
    expect(screen.getByTestId('game-log')).toBeDefined();
  });

  it('swaps quiet state and stack panel as fresh views arrive (pure render)', () => {
    const { rerender } = render(<Rail view={viewWith([])} />);
    expect(screen.getByTestId('stack-quiet')).toBeDefined();
    rerender(<Rail view={viewWith(twoSpells)} />);
    expect(screen.queryByTestId('stack-quiet')).toBeNull();
    expect(screen.getByTestId('stack-item-s2')).toBeDefined();
    // Emptying the stack returns the quiet state — the rail itself never left.
    rerender(<Rail view={viewWith([])} />);
    expect(screen.getByTestId('stack-quiet')).toBeDefined();
    expect(screen.getByTestId('rail')).toBeDefined();
  });

  it('forwards a log reference click to onHighlight', () => {
    const onHighlight = vi.fn();
    render(<Rail view={viewWith([], oneLog)} onHighlight={onHighlight} />);
    fireEvent.click(screen.getByTestId('log-ref-s1'));
    expect(onHighlight).toHaveBeenCalledWith('s1');
  });

  it('keeps stack objects pickable in targeting mode inside the rail', () => {
    const onPick = vi.fn();
    render(<Rail view={viewWith(twoSpells)} targeting={{ candidates: ['s1'], onPick }} />);
    fireEvent.click(screen.getByTestId('target-s1'));
    expect(onPick).toHaveBeenCalledWith('s1');
  });

  it('keeps stack objects inspectable in the rail', () => {
    const onInspect = vi.fn();
    render(<Rail view={viewWith(twoSpells)} onInspect={onInspect} />);
    fireEvent.click(screen.getByTestId('inspect-s2'));
    expect(onInspect).toHaveBeenCalledWith('s2');
  });
});
