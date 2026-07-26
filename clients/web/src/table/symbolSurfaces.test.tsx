/**
 * Symbol notation at the shell's text surfaces (issue #462).
 *
 * The vocabulary itself is proven in `chrome/symbols`; these are the **call
 * sites** — the defect this issue records was not a missing tokenizer but that
 * every DOM surface printed the server's braces verbatim. One assertion per
 * surface, each stating the two halves of the contract: no brace notation
 * survives into what the player reads, and the symbol still announces its
 * meaning to a screen reader.
 *
 * jsdom draws nothing, so nothing here claims a rendered symbol looks right —
 * only which nodes exist and what they announce.
 */
import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { GameView, StackItem } from '../protocol';
import { ControlButton, ManaReservoir } from './controls';
import { DecisionArea } from './decision';
import type { DecisionSurface } from './decision';
import { StackStage } from './stack';

afterEach(cleanup);

/** The mana ability every surface below is asked to show. */
const ABILITY = '{T}: Add {G}.';

/** A decision surface whose title and sentence both carry notation. */
const SURFACE: DecisionSurface = {
  kind: 'multiSelect',
  title: ABILITY,
  prompt: `Pay ${ABILITY}`,
  confirm: false,
  advance: false,
  undo: false,
  cancel: false,
};

/** Every handler the area needs; none of these tests presses anything. */
const NOOPS = {
  onConfirm: vi.fn(),
  onAdvance: vi.fn(),
  onUndo: vi.fn(),
  onCancel: vi.fn(),
  onToggleRow: vi.fn(),
  onMoveRow: vi.fn(),
  onChooseOption: vi.fn(),
};

function viewWith(stack: StackItem[] = []): GameView {
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
  };
}

/** The accessible names of every symbol drawn inside an element, in order. */
function symbols(host: HTMLElement): (string | null)[] {
  return Array.from(host.querySelectorAll('[data-symbol]')).map((el) =>
    el.getAttribute('aria-label'),
  );
}

describe('symbol notation reaches no player-visible surface (issue #462)', () => {
  it('draws an action control’s label and speaks the spoken form', () => {
    render(<ControlButton variant="secondary" label={ABILITY} onPress={vi.fn()} testId="c" />);
    const button = screen.getByTestId('c');
    expect(button.textContent).not.toContain('{');
    expect(symbols(button)).toEqual(['tap', 'green mana']);
    // The drawn letters are `role="img"`, so the button's own name must be the
    // substitution or a reader would hear a label with holes in it.
    expect(screen.getByRole('button', { name: 'tap: Add green mana.' })).toBeTruthy();
  });

  it('leaves a symbol-free label as its own accessible name (no gratuitous aria)', () => {
    render(<ControlButton variant="primary" label="PASS PRIORITY" onPress={vi.fn()} testId="p" />);
    expect(screen.getByTestId('p').getAttribute('aria-label')).toBeNull();
    expect(screen.getByRole('button', { name: 'PASS PRIORITY' })).toBeTruthy();
  });

  it('draws the decision area’s question, and draws it exactly once', () => {
    render(<DecisionArea surface={SURFACE} {...NOOPS} />);
    const area = screen.getByTestId('decision-area');
    expect(area.textContent).not.toContain('{');
    // Title then sentence — two runs of the same notation, and no third copy
    // anywhere: the strip and the sheet that used to restate it are gone.
    expect(symbols(area)).toEqual(['tap', 'green mana', 'tap', 'green mana']);
    expect(document.querySelectorAll('[data-decision-prompt]')).toHaveLength(1);
  });

  it('draws the mana reservoir’s pool and speaks each symbol’s name', () => {
    render(<ManaReservoir pool={['{G}', '{U}']} />);
    const reservoir = screen.getByTestId('mana-reservoir');
    expect(reservoir.textContent).not.toContain('{');
    expect(symbols(reservoir)).toEqual(['green mana', 'blue mana']);
    expect(screen.getByRole('group', { name: 'Mana pool: green mana, blue mana' })).toBeTruthy();
  });

  it('prints an unknown mana code literally rather than dropping it', () => {
    render(<ManaReservoir pool={['{G}', '{WEIRD}']} />);
    expect(screen.getByTestId('mana-reservoir').textContent).toContain('{WEIRD}');
  });

  it('draws a stack entry’s server description', () => {
    render(<StackStage view={viewWith([{ id: 's1', controller: 'p1', description: ABILITY }])} />);
    const entry = screen.getByTestId('stack-entry-s1');
    expect(entry.textContent).not.toContain('{');
    expect(symbols(entry)).toEqual(['tap', 'green mana']);
    // §9.2's accessible name is a pure-text context: it speaks the symbols.
    expect(entry.getAttribute('aria-label')).toContain('tap: Add green mana.');
  });

  it('speaks the decision area’s group name from its server title', () => {
    render(<DecisionArea surface={SURFACE} {...NOOPS} testId="area" />);
    const title = screen.getByTestId('area-title');
    expect(title.textContent).not.toContain('{');
    expect(symbols(title)).toEqual(['tap', 'green mana']);
    expect(screen.getByRole('group', { name: 'tap: Add green mana.' })).toBeTruthy();
  });

  it('never crashes or hides an unrecognized code at any surface', () => {
    render(<ControlButton variant="secondary" label="Pay {WEIRD}" onPress={vi.fn()} testId="u" />);
    expect(screen.getByTestId('u').textContent).toContain('Pay {WEIRD}');
  });
});
