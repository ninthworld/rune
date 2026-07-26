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
import { ControlButton } from './controls';
import { DecisionPlaque } from './decision';
import type { PlaquePlacement } from './decision';
import { PromptStrip } from './PromptStrip';
import { StackStage } from './stack';

afterEach(cleanup);

/** The mana ability every surface below is asked to show. */
const ABILITY = '{T}: Add {G}.';

const PLACEMENT: PlaquePlacement = {
  rect: { x: 100, y: 100, w: 272, h: 102 },
  form: 'anchored',
  side: 'below',
  slide: 0,
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

  it('draws the prompt strip’s action label and the stack top it names', () => {
    render(
      <PromptStrip
        view={viewWith([{ id: 's1', controller: 'p1', description: ABILITY }])}
        prompt={{ kind: 'priority' } as never}
        multiSelect={{ label: ABILITY, prompt: 'Choose lands', step: 1, total: 1, chosen: 0 }}
      />,
    );
    const strip = screen.getByTestId('prompt-banner');
    expect(strip.textContent).not.toContain('{');
    expect(symbols(strip)).toEqual(['tap', 'green mana']);
  });

  it('draws a stack entry’s server description', () => {
    render(<StackStage view={viewWith([{ id: 's1', controller: 'p1', description: ABILITY }])} />);
    const entry = screen.getByTestId('stack-entry-s1');
    expect(entry.textContent).not.toContain('{');
    expect(symbols(entry)).toEqual(['tap', 'green mana']);
    // §9.2's accessible name is a pure-text context: it speaks the symbols.
    expect(entry.getAttribute('aria-label')).toContain('tap: Add green mana.');
  });

  it('draws the decision plaque’s title and speaks its group name', () => {
    render(<DecisionPlaque title={ABILITY} placement={PLACEMENT} testId="plaque" />);
    const title = screen.getByTestId('plaque-title');
    expect(title.textContent).not.toContain('{');
    expect(symbols(title)).toEqual(['tap', 'green mana']);
    expect(screen.getByRole('group', { name: 'tap: Add green mana.' })).toBeTruthy();
  });

  it('never crashes or hides an unrecognized code at any surface', () => {
    render(<ControlButton variant="secondary" label="Pay {WEIRD}" onPress={vi.fn()} testId="u" />);
    expect(screen.getByTestId('u').textContent).toContain('Pay {WEIRD}');
  });
});
