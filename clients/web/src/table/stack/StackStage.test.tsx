import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import type { GameView, Permanent, StackItem } from '../../protocol';
import { StackStage } from './StackStage';

afterEach(cleanup);

function viewWith(stack: StackItem[], overrides: Partial<GameView> = {}): GameView {
  return {
    you: 'p1',
    my_hand: [],
    me: { life: 20, library_size: 40 },
    opponents: [{ player_id: 'p2', life: 20, library_size: 40, hand_size: 7, graveyard_size: 0 }],
    battlefield: [],
    stack,
    graveyards: [],
    exile: [],
    phase: 'precombat_main',
    turn: 1,
    active_player: 'p1',
    seat_order: ['p1', 'p2'],
    mana_pool: [],
    valid_actions: [],
    player_names: { p1: 'Imogen', p2: 'Sorel' },
    commander_damage: [],
    ...overrides,
  };
}

function spells(n: number): StackItem[] {
  return Array.from({ length: n }, (_, i) => ({
    id: `s${i + 1}`,
    controller: 'p2',
    description: `Spell ${i + 1}`,
  }));
}

function permanent(id: string, name: string): Permanent {
  return { id, controller: 'p1', owner: 'p1', card: { id, name, type_line: 'Creature' } };
}

describe('StackStage — the empty stack costs nothing', () => {
  it('renders no node at all when the stack is empty', () => {
    const { container } = render(<StackStage view={viewWith([])} />);
    expect(screen.queryByTestId('stack-stage')).toBeNull();
    expect(container.firstChild).toBeNull();
  });

  it('renders no node on compact geometry either — not even a handle', () => {
    render(<StackStage view={viewWith([])} compact />);
    expect(screen.queryByTestId('stack-sheet-handle')).toBeNull();
    expect(screen.queryByTestId('stack-stage')).toBeNull();
  });

  it('appears on its own the moment the stack is non-empty', () => {
    render(<StackStage view={viewWith(spells(1))} />);
    expect(screen.getByTestId('stack-stage')).toBeTruthy();
  });
});

describe('StackStage — what a reader can tell at a glance', () => {
  it('lists entries top-first with the top marked "Resolves next"', () => {
    render(<StackStage view={viewWith(spells(3))} />);
    const items = within(screen.getByTestId('stack-stage-list')).getAllByRole('listitem');
    expect(items).toHaveLength(3);
    expect(items[0].textContent).toContain('Spell 3');
    expect(screen.getByTestId('stack-top-s3').textContent).toBe('Resolves next');
    expect(screen.queryByTestId('stack-top-s2')).toBeNull();
  });

  it('names the stage and every entry for a screen reader', () => {
    render(<StackStage view={viewWith(spells(2))} />);
    expect(screen.getByLabelText('Stack, 2 objects, top resolves first')).toBeTruthy();
    expect(screen.getByTestId('stack-entry-s2').getAttribute('aria-label')).toContain(
      '1 of 2. Resolves next.',
    );
  });

  it('distinguishes an ability from a spell and names its source', () => {
    const view = viewWith(
      [{ id: 'a1', controller: 'p1', description: 'Tap: add {G}', source: 'perm1' }],
      { battlefield: [permanent('perm1', 'Ridge Wolf')] },
    );
    render(<StackStage view={view} />);
    expect(screen.getByTestId('stack-subtitle-a1').textContent).toBe('ability — Ridge Wolf · You');
    const slot = screen.getByTestId('stack-entry-a1').closest('li');
    expect(slot?.getAttribute('data-kind')).toBe('ability');
  });
});

describe('StackStage — condensation at depth', () => {
  it('draws a pile below the collapse point and a rail at or above it', () => {
    const { rerender } = render(<StackStage view={viewWith(spells(5))} />);
    expect(screen.getByTestId('stack-stage-list').getAttribute('data-layout')).toBe('pile');
    rerender(<StackStage view={viewWith(spells(6))} />);
    expect(screen.getByTestId('stack-stage-list').getAttribute('data-layout')).toBe('rail');
  });

  it('condenses to compact rows rather than growing without bound', () => {
    render(<StackStage view={viewWith(spells(9))} />);
    const items = within(screen.getByTestId('stack-stage-list')).getAllByRole('listitem');
    expect(items[0].getAttribute('data-tier')).toBe('expanded');
    expect(items.slice(1).every((li) => li.getAttribute('data-tier') === 'row')).toBe(true);
  });

  it('states the hidden count instead of letting the scroll swallow it', () => {
    render(<StackStage view={viewWith(spells(12))} />);
    expect(screen.getByTestId('stack-more').textContent).toBe('+4 more');
    expect(screen.getByTestId('stack-stage-list').getAttribute('data-scrolls')).toBe('true');
  });

  it('draws no "+K more" while everything fits', () => {
    render(<StackStage view={viewWith(spells(8))} />);
    expect(screen.queryByTestId('stack-more')).toBeNull();
  });
});

describe('StackStage — carried behaviour from the shipped StackPanel', () => {
  it('makes only server-listed candidates pickable, and picks by id', () => {
    const onPick = vi.fn();
    render(<StackStage view={viewWith(spells(3))} targeting={{ candidates: ['s2'], onPick }} />);
    expect(screen.queryByTestId('target-s1')).toBeNull();
    expect(screen.queryByTestId('target-s3')).toBeNull();
    fireEvent.click(screen.getByTestId('target-s2'));
    expect(onPick).toHaveBeenCalledWith('s2');
  });

  it('keeps the inspect handle outside the candidate button, for every entry', () => {
    const onInspect = vi.fn();
    render(
      <StackStage
        view={viewWith(spells(2))}
        targeting={{ candidates: ['s2'], onPick: vi.fn() }}
        onInspect={onInspect}
      />,
    );
    const handle = screen.getByTestId('inspect-s2');
    expect(handle.closest('button[data-testid="target-s2"]')).toBeNull();
    fireEvent.click(handle);
    expect(onInspect).toHaveBeenCalledWith('s2');
    // And on a non-candidate entry too.
    fireEvent.click(screen.getByTestId('inspect-s1'));
    expect(onInspect).toHaveBeenCalledWith('s1');
  });

  it('renders no inspect handle when the host offers no inspect surface', () => {
    render(<StackStage view={viewWith(spells(1))} />);
    expect(screen.queryByTestId('inspect-s1')).toBeNull();
  });
});

describe('StackStage — the stage renders no game control (§1.4, D17)', () => {
  it('has no RESOLVE, RESPOND, or pass control of its own', () => {
    render(<StackStage view={viewWith(spells(3))} onInspect={vi.fn()} />);
    const stage = screen.getByTestId('stack-stage');
    for (const button of within(stage).getAllByRole('button')) {
      const name = `${button.getAttribute('aria-label') ?? ''} ${button.textContent ?? ''}`;
      expect(name).not.toMatch(/resolve|respond|pass/i);
    }
  });
});

describe('StackStage — keyboard (§9.3)', () => {
  it('is one tab stop, with the arrows walking the entries', () => {
    render(<StackStage view={viewWith(spells(3))} />);
    const list = screen.getByTestId('stack-stage-list');
    const focusables = within(list)
      .getAllByRole('listitem')
      .map((li) => li.querySelector('[tabindex]'));
    expect(focusables.filter((el) => el?.getAttribute('tabindex') === '0')).toHaveLength(1);

    fireEvent.keyDown(list, { key: 'ArrowDown' });
    expect(document.activeElement).toBe(screen.getByTestId('stack-entry-s2'));
    fireEvent.keyDown(list, { key: 'End' });
    expect(document.activeElement).toBe(screen.getByTestId('stack-entry-s1'));
    fireEvent.keyDown(list, { key: 'Home' });
    expect(document.activeElement).toBe(screen.getByTestId('stack-entry-s3'));
  });

  it('promotes the focused entry to the Expanded tier and demotes the top', () => {
    render(<StackStage view={viewWith(spells(4))} />);
    const list = screen.getByTestId('stack-stage-list');
    fireEvent.keyDown(list, { key: 'End' });
    const items = within(list).getAllByRole('listitem');
    expect(items[items.length - 1].getAttribute('data-tier')).toBe('expanded');
    expect(items[0].getAttribute('data-tier')).not.toBe('expanded');
  });
});

describe('StackStage — the compact sheet (§10.4)', () => {
  it('opens on its own when the receiver holds priority', () => {
    render(<StackStage view={viewWith(spells(8), { priority_player: 'p1' })} compact />);
    const stage = screen.getByTestId('stack-stage');
    expect(stage.getAttribute('data-layout')).toBe('sheet');
    expect(within(stage).getAllByRole('listitem')).toHaveLength(8);
  });

  it('stays a handle when the receiver does not hold priority', () => {
    render(<StackStage view={viewWith(spells(8), { priority_player: 'p2' })} compact />);
    expect(screen.queryByTestId('stack-stage')).toBeNull();
    const handle = screen.getByTestId('stack-sheet-handle');
    expect(handle.textContent).toContain('STACK (8)');
    // The handle still states what resolves next, so the collapsed form is not mute.
    expect(handle.textContent).toContain('Spell 8');
  });

  it('opens from the handle and collapses again without touching game state', () => {
    render(<StackStage view={viewWith(spells(8), { priority_player: 'p2' })} compact />);
    fireEvent.click(screen.getByTestId('stack-sheet-handle'));
    expect(within(screen.getByTestId('stack-stage')).getAllByRole('listitem')).toHaveLength(8);
    fireEvent.click(screen.getByTestId('stack-sheet-dismiss'));
    expect(screen.getByTestId('stack-sheet-handle')).toBeTruthy();
  });

  it('is readable at depth 8: every row is an entry and nothing is dropped', () => {
    render(<StackStage view={viewWith(spells(8), { priority_player: 'p1' })} compact />);
    const items = within(screen.getByTestId('stack-stage-list')).getAllByRole('listitem');
    expect(items.map((li) => li.getAttribute('data-tier'))).toEqual([
      'expanded',
      'row',
      'row',
      'row',
      'row',
      'row',
      'row',
      'row',
    ]);
  });
});

describe('StackStage — reconstructable from one view', () => {
  it('renders identically from a fresh mount and from a re-render of the same view', () => {
    const view = viewWith(spells(6), { priority_player: 'p1' });
    const first = render(<StackStage view={view} onInspect={vi.fn()} />);
    const before = first.container.innerHTML;
    cleanup();
    const second = render(<StackStage view={view} onInspect={vi.fn()} />);
    expect(second.container.innerHTML).toBe(before);
  });
});
