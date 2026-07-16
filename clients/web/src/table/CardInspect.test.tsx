import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import type { CardView, StackItem } from '../protocol';
import { CardInspect } from './CardInspect';

afterEach(cleanup);

describe('CardInspect (issue #261)', () => {
  it('renders every CardView field the server sent — and nothing derived', () => {
    const card: CardView = {
      id: 'c1',
      name: 'Serra Angel',
      type_line: 'Creature — Angel',
      mana_cost: '{3}{W}{W}',
      rules_text: 'Vigilance',
      power: '4',
      toughness: '4',
      keywords: ['flying', 'first_strike'],
    };
    render(<CardInspect target={{ kind: 'card', card }} onClose={vi.fn()} />);

    expect(screen.getByTestId('card-inspect-name').textContent).toBe('Serra Angel');
    expect(screen.getByTestId('card-inspect-cost').textContent).toBe('{3}{W}{W}');
    expect(screen.getByTestId('card-inspect-type').textContent).toContain('Angel');
    expect(screen.getByTestId('card-inspect-pt').textContent).toBe('4/4');
    expect(screen.getByTestId('card-inspect-rules').textContent).toContain('Vigilance');
    // Keywords are shown title-cased from their lowercase wire names.
    const keywords = screen.getByTestId('card-inspect-keywords');
    expect(keywords.textContent).toContain('Flying');
    expect(keywords.textContent).toContain('First Strike');
  });

  it('shows a placeholder when a card has no rules text, and omits absent fields', () => {
    const card: CardView = { id: 'l1', name: 'Forest', type_line: 'Basic Land — Forest' };
    render(<CardInspect target={{ kind: 'card', card }} onClose={vi.fn()} />);

    expect(screen.getByTestId('card-inspect-rules').textContent).toMatch(/no rules text/i);
    // No mana cost, no P/T, no keywords for a basic land.
    expect(screen.queryByTestId('card-inspect-cost')).toBeNull();
    expect(screen.queryByTestId('card-inspect-pt')).toBeNull();
    expect(screen.queryByTestId('card-inspect-keywords')).toBeNull();
  });

  it("shows a permanent's dynamic state (tapped, counters)", () => {
    const card: CardView = {
      id: 'p1',
      name: 'Grizzly Bears',
      type_line: 'Creature — Bear',
      power: '2',
      toughness: '2',
    };
    render(
      <CardInspect
        target={{ kind: 'card', card, tapped: true, counters: [{ kind: '+1/+1', count: 2 }] }}
        onClose={vi.fn()}
      />,
    );
    const state = screen.getByTestId('card-inspect-state');
    expect(state.textContent).toContain('Tapped');
    expect(state.textContent).toContain('2× +1/+1');
  });

  it('inspects a stack object from its server-composed description', () => {
    const item: StackItem = { id: 's1', controller: 'p2', description: 'Lightning Bolt → p1' };
    render(<CardInspect target={{ kind: 'stack', item }} onClose={vi.fn()} />);
    expect(screen.getByTestId('card-inspect-name').textContent).toBe('Lightning Bolt → p1');
    expect(screen.getByTestId('card-inspect-rules').textContent).toContain('Lightning Bolt → p1');
    expect(screen.getByTestId('card-inspect-state').textContent).toContain('Controller p2');
  });

  it('closes on the close control and on a backdrop click', () => {
    const onClose = vi.fn();
    const card: CardView = { id: 'c1', name: 'Opt', type_line: 'Instant' };
    render(<CardInspect target={{ kind: 'card', card }} onClose={onClose} />);

    fireEvent.click(screen.getByTestId('card-inspect-close'));
    expect(onClose).toHaveBeenCalledTimes(1);

    fireEvent.click(screen.getByTestId('card-inspect-backdrop'));
    expect(onClose).toHaveBeenCalledTimes(2);
  });

  it('does not close when the panel body itself is clicked', () => {
    const onClose = vi.fn();
    const card: CardView = { id: 'c1', name: 'Opt', type_line: 'Instant' };
    render(<CardInspect target={{ kind: 'card', card }} onClose={onClose} />);
    fireEvent.click(screen.getByTestId('card-inspect'));
    expect(onClose).not.toHaveBeenCalled();
  });
});
