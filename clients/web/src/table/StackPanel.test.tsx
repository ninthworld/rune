import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import type { GameView, Permanent, StackItem } from '../protocol';
import { StackPanel } from './StackPanel';

afterEach(cleanup);

/**
 * A minimal live {@link GameView} carrying just the fields the stack panel reads.
 * Callers override `stack` (and `battlefield` for ability-source resolution); the
 * rest are the empty/defaulted collections a real normalized view always has.
 */
function viewWith(stack: StackItem[], battlefield: Permanent[] = []): GameView {
  return {
    you: 'p1',
    my_hand: [],
    me: { life: 20, library_size: 40 },
    opponents: [],
    battlefield,
    stack,
    graveyards: [],
    exile: [],
    phase: 'precombat_main',
    mana_pool: [],
    valid_actions: [],
  };
}

/** A battlefield permanent, minimally, so an ability's `source` resolves to a name. */
function permanent(id: string, name: string): Permanent {
  return {
    id,
    controller: 'p1',
    owner: 'p1',
    card: { id, name, type_line: 'Creature' },
  };
}

describe('StackPanel (issue #142)', () => {
  it('renders no chrome for an empty stack', () => {
    render(<StackPanel view={viewWith([])} />);
    expect(screen.queryByTestId('stack-panel')).toBeNull();
  });

  it('renders a multi-object stack top-first with the top clearly distinguished', () => {
    // Wire order is bottom-first: s1 was put on first (bottom), s2 last (top).
    const view = viewWith([
      { id: 's1', controller: 'p1', description: 'Grizzly Bears' },
      { id: 's2', controller: 'p2', description: 'Counterspell' },
    ]);
    render(<StackPanel view={view} />);

    const panel = screen.getByTestId('stack-panel');
    expect(within(panel).getByRole('heading').textContent).toContain('Stack (2)');

    // Both objects render...
    expect(screen.getByTestId('stack-item-s1')).toBeDefined();
    expect(screen.getByTestId('stack-item-s2')).toBeDefined();

    // ...the top of the stack (s2, added last) is the one flagged "resolves next"...
    expect(screen.getByTestId('stack-top-s2')).toBeDefined();
    expect(screen.queryByTestId('stack-top-s1')).toBeNull();

    // ...and it is shown first (top-first) so the resolving object reads at the top.
    const items = within(panel).getAllByRole('listitem');
    expect(items[0].textContent).toContain('Counterspell');
    expect(items[1].textContent).toContain('Grizzly Bears');

    // Controllers are shown per entry.
    expect(within(screen.getByTestId('stack-item-s2')).getByText(/Controller p2/)).toBeDefined();
    expect(within(screen.getByTestId('stack-item-s1')).getByText(/Controller p1/)).toBeDefined();
  });

  it('marks a spell as a spell and shows no source line', () => {
    render(<StackPanel view={viewWith([{ id: 's1', controller: 'p1', description: 'Shock' }])} />);
    expect(within(screen.getByTestId('stack-item-s1')).getByText('Spell')).toBeDefined();
    expect(screen.queryByTestId('stack-source-s1')).toBeNull();
  });

  it('renders an ability entry tied to its source permanent by name', () => {
    const view = viewWith(
      [{ id: 's1', controller: 'p1', description: 'Add {G}', source: 'perm_elf' }],
      [permanent('perm_elf', 'Llanowar Elves')],
    );
    render(<StackPanel view={view} />);

    const entry = screen.getByTestId('stack-item-s1');
    expect(within(entry).getByText('Ability')).toBeDefined();
    // The source resolves to the permanent's display name, not the raw id.
    expect(screen.getByTestId('stack-source-s1').textContent).toContain('Llanowar Elves');
  });

  it('falls back to the raw source id when the source is not a visible permanent', () => {
    const view = viewWith([
      { id: 's1', controller: 'p1', description: 'Trigger', source: 'perm_gone' },
    ]);
    render(<StackPanel view={view} />);
    expect(screen.getByTestId('stack-source-s1').textContent).toContain('perm_gone');
  });

  it('shows the description verbatim (server bakes chosen targets into it)', () => {
    const view = viewWith([
      { id: 's1', controller: 'p1', description: 'Lightning Bolt → Grizzly Bears' },
    ]);
    render(<StackPanel view={view} />);
    expect(within(screen.getByTestId('stack-item-s1')).getByText(/→ Grizzly Bears/)).toBeDefined();
  });

  it('renders an inspect handle per entry and reports the id (issue #261)', () => {
    const onInspect = vi.fn();
    const view = viewWith([
      { id: 's1', controller: 'p1', description: 'Grizzly Bears' },
      { id: 's2', controller: 'p2', description: 'Counterspell' },
    ]);
    render(<StackPanel view={view} onInspect={onInspect} />);
    fireEvent.click(screen.getByTestId('inspect-s2'));
    expect(onInspect).toHaveBeenCalledWith('s2');
  });

  it('inspects a candidate entry without picking it as a target (issue #261)', () => {
    const onPick = vi.fn();
    const onInspect = vi.fn();
    const view = viewWith([{ id: 's1', controller: 'p1', description: 'Grizzly Bears' }]);
    render(
      <StackPanel view={view} targeting={{ candidates: ['s1'], onPick }} onInspect={onInspect} />,
    );
    // The inspect handle is a sibling of the target button (valid HTML, no nesting).
    fireEvent.click(screen.getByTestId('inspect-s1'));
    expect(onInspect).toHaveBeenCalledWith('s1');
    expect(onPick).not.toHaveBeenCalled();
  });

  it('makes a candidate stack object pickable in targeting mode and picks it', () => {
    const onPick = vi.fn();
    const view = viewWith([
      { id: 's1', controller: 'p1', description: 'Grizzly Bears' },
      { id: 's2', controller: 'p2', description: 'Counterspell' },
    ]);
    // The server lists s1 (a spell on the stack) as the only legal target.
    render(<StackPanel view={view} targeting={{ candidates: ['s1'], onPick }} />);

    // The candidate is a pickable button; the non-candidate stays inert.
    const target = screen.getByTestId('target-s1');
    expect(screen.queryByTestId('stack-item-s1')).toBeNull();
    expect(screen.getByTestId('stack-item-s2')).toBeDefined();

    fireEvent.click(target);
    expect(onPick).toHaveBeenCalledTimes(1);
    expect(onPick).toHaveBeenCalledWith('s1');
  });
});
