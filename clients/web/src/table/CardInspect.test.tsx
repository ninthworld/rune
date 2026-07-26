import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import type { CardView, StackItem } from '../protocol';
import { CardInspect } from './CardInspect';

afterEach(cleanup);

/** The brought-forward card face inside the surface, by its accessible name. */
function face(name: string): HTMLElement {
  return within(screen.getByTestId('card-inspect')).getByRole('img', { name });
}

describe('CardInspect (issues #261, #569)', () => {
  it('brings the real card face forward, carrying every field the server sent', () => {
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

    // The surface is the shared card renderer at its reading tier — not a text
    // panel that re-lists the card in its own typography (issue #569).
    const drawn = face('Serra Angel');
    expect(drawn.getAttribute('data-tier')).toBe('inspect');
    expect(drawn.textContent).toContain('Creature — Angel');
    expect(drawn.textContent).toContain('Vigilance');
    expect(drawn.textContent).toContain('4/4');
    // Issue #462: the cost is the server's string drawn as symbols — the player
    // never sees brace notation, and each symbol announces its own name.
    expect(drawn.textContent).not.toContain('{');
    expect(
      within(drawn)
        .getAllByRole('img')
        .map((pip) => pip.getAttribute('aria-label')),
    ).toEqual(expect.arrayContaining(['three generic mana', 'white mana']));

    // The annex carries what a printed face has no home for: the keyword names
    // spelled out (the face draws capped glyph plates).
    const keywords = screen.getByTestId('card-inspect-keywords');
    expect(keywords.textContent).toContain('Flying');
    expect(keywords.textContent).toContain('First Strike');
  });

  it('omits absent fields rather than inventing them', () => {
    const card: CardView = { id: 'l1', name: 'Forest', type_line: 'Basic Land — Forest' };
    render(<CardInspect target={{ kind: 'card', card }} onClose={vi.fn()} />);

    const drawn = face('Forest');
    expect(drawn.textContent).toContain('Basic Land — Forest');
    expect(screen.queryByTestId('card-inspect-keywords')).toBeNull();
    expect(screen.queryByTestId('card-inspect-state')).toBeNull();
  });

  it("shows a permanent's dynamic state, drawn AND spelled out", () => {
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
    // The face itself is tapped and countered, exactly as it is on the board…
    const drawn = face('Grizzly Bears');
    expect(drawn.getAttribute('data-tapped')).toBe('true');
    // …and the annex says it in words, so the channel is never visual alone.
    const state = screen.getByTestId('card-inspect-state');
    expect(state.textContent).toContain('Tapped');
    expect(state.textContent).toContain('2× +1/+1');
  });

  it('shows the attachment relationship from both sides (issue #333)', () => {
    const aura: CardView = { id: 'aura', name: 'Ironbark Aegis', type_line: 'Enchantment — Aura' };
    // The aura names the host it enchants.
    render(
      <CardInspect
        target={{ kind: 'card', card: aura, attachedTo: { id: 'bear', name: 'Grizzly Bears' } }}
        onClose={vi.fn()}
      />,
    );
    expect(screen.getByTestId('card-inspect-attachments').textContent).toContain(
      'Attached to Grizzly Bears',
    );
    cleanup();

    // The host lists what is attached to it.
    const bear: CardView = { id: 'bear', name: 'Grizzly Bears', type_line: 'Creature — Bear' };
    render(
      <CardInspect
        target={{ kind: 'card', card: bear, attachments: [{ id: 'aura', name: 'Ironbark Aegis' }] }}
        onClose={vi.fn()}
      />,
    );
    expect(screen.getByTestId('card-inspect-attachments').textContent).toContain(
      'Enchanted by Ironbark Aegis',
    );
  });

  it('omits the attachment row for an unattached, unenchanted permanent (issue #333)', () => {
    const card: CardView = { id: 'p1', name: 'Grizzly Bears', type_line: 'Creature — Bear' };
    render(<CardInspect target={{ kind: 'card', card }} onClose={vi.fn()} />);
    expect(screen.queryByTestId('card-inspect-attachments')).toBeNull();
  });

  it("draws a stack object's card face and its server-composed description", () => {
    const card: CardView = { id: 'c9', name: 'Lightning Bolt', type_line: 'Instant' };
    const item: StackItem = {
      id: 's1',
      controller: 'p2',
      description: 'Lightning Bolt → p1',
      kind: 'spell',
      card,
    };
    render(<CardInspect target={{ kind: 'stack', item }} onClose={vi.fn()} />);
    expect(face('Lightning Bolt').getAttribute('data-tier')).toBe('inspect');
    expect(screen.getByTestId('card-inspect-stack-kind').textContent).toBe('Spell on the stack');
    expect(screen.getByTestId('card-inspect-description').textContent).toContain(
      'Lightning Bolt → p1',
    );
    expect(screen.getByTestId('card-inspect-state').textContent).toContain('Controller p2');
  });

  it('degrades to a plate when a stack object has no face to draw (CR 608.2)', () => {
    const item: StackItem = { id: 's1', controller: 'p2', description: 'Draw a card' };
    render(<CardInspect target={{ kind: 'stack', item }} onClose={vi.fn()} />);
    expect(screen.getByTestId('card-inspect-faceless').textContent).toContain('Draw a card');
    expect(screen.getByTestId('card-inspect-stack-kind').textContent).toBe('On the stack');
    expect(screen.getByTestId('card-inspect-description').textContent).toContain('Draw a card');
  });

  it('names an ability by the provenance the server stated (issue #579)', () => {
    for (const [kind, text] of [
      ['triggered', 'Triggered ability on the stack'],
      ['activated', 'Activated ability on the stack'],
      ['ability', 'Ability on the stack'],
    ] as const) {
      render(
        <CardInspect
          target={{ kind: 'stack', item: { id: 's', controller: 'p1', description: 'x', kind } }}
          onClose={vi.fn()}
        />,
      );
      expect(screen.getByTestId('card-inspect-stack-kind').textContent).toBe(text);
      cleanup();
    }
  });

  it('closes on the close control and on the veil', () => {
    const onClose = vi.fn();
    const card: CardView = { id: 'c1', name: 'Opt', type_line: 'Instant' };
    render(<CardInspect target={{ kind: 'card', card }} onClose={onClose} />);

    fireEvent.click(screen.getByTestId('card-inspect-close'));
    expect(onClose).toHaveBeenCalledTimes(1);

    fireEvent.click(screen.getByTestId('card-inspect-backdrop'));
    expect(onClose).toHaveBeenCalledTimes(2);
  });

  it('does not close when the card itself is clicked', () => {
    const onClose = vi.fn();
    const card: CardView = { id: 'c1', name: 'Opt', type_line: 'Instant' };
    render(<CardInspect target={{ kind: 'card', card }} onClose={onClose} />);
    fireEvent.click(screen.getByTestId('card-inspect'));
    expect(onClose).not.toHaveBeenCalled();
  });

  it('renders a transient peek as a non-blocking preview (issue #321)', () => {
    const card: CardView = {
      id: 'c1',
      name: 'Grizzly Bears',
      type_line: 'Creature — Bear',
      power: '2',
      toughness: '2',
    };
    render(<CardInspect target={{ kind: 'card', card }} onClose={vi.fn()} transient />);
    const preview = screen.getByTestId('card-inspect');
    // Same card, same renderer, but no modal chrome: no veil, no close control —
    // a peek can never block the interaction the player is mid-way through.
    expect(preview.getAttribute('data-inspect')).toBe('peek');
    expect(preview.getAttribute('data-transient')).toBe('true');
    expect(face('Grizzly Bears')).toBeDefined();
    expect(screen.queryByTestId('card-inspect-backdrop')).toBeNull();
    expect(screen.queryByTestId('card-inspect-close')).toBeNull();
  });

  it('keeps the three states distinct on the surface itself', () => {
    const card: CardView = { id: 'c1', name: 'Opt', type_line: 'Instant' };
    render(<CardInspect target={{ kind: 'card', card }} onClose={vi.fn()} />);
    expect(screen.getByTestId('card-inspect').getAttribute('data-inspect')).toBe('pinned');
    cleanup();

    render(<CardInspect target={{ kind: 'card', card }} onClose={vi.fn()} transient />);
    expect(screen.getByTestId('card-inspect').getAttribute('data-inspect')).toBe('peek');
    cleanup();

    render(<CardInspect target={{ kind: 'card', card }} onClose={vi.fn()} deferring />);
    expect(screen.getByTestId('card-inspect').getAttribute('data-inspect')).toBe('deferred');
  });

  it('never covers an open decision or its candidates', () => {
    const onClose = vi.fn();
    const card: CardView = { id: 'c1', name: 'Opt', type_line: 'Instant' };
    render(<CardInspect target={{ kind: 'card', card }} onClose={onClose} deferring />);

    const veil = screen.getByTestId('card-inspect-backdrop');
    // The dismiss veil is gone, so nothing paints over the decision and nothing
    // eats a click aimed at a candidate underneath it.
    expect(veil.getAttribute('data-deferring')).toBe('true');
    fireEvent.click(veil);
    expect(onClose).not.toHaveBeenCalled();
    // The surface stops claiming the whole screen as a modal…
    expect(screen.getByTestId('card-inspect').getAttribute('aria-modal')).toBeNull();
    // …but the card is still explicitly dismissible.
    fireEvent.click(screen.getByTestId('card-inspect-close'));
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it('offers the art settings only from the pinned surface', () => {
    const onOpenArtSettings = vi.fn();
    const card: CardView = { id: 'c1', name: 'Opt', type_line: 'Instant' };
    render(
      <CardInspect
        target={{ kind: 'card', card }}
        onClose={vi.fn()}
        onOpenArtSettings={onOpenArtSettings}
      />,
    );
    fireEvent.click(screen.getByTestId('card-inspect-art-settings'));
    expect(onOpenArtSettings).toHaveBeenCalledOnce();
    cleanup();

    // A peek takes no input at all, so it offers no control.
    render(
      <CardInspect
        target={{ kind: 'card', card }}
        onClose={vi.fn()}
        onOpenArtSettings={onOpenArtSettings}
        transient
      />,
    );
    expect(screen.queryByTestId('card-inspect-art-settings')).toBeNull();
  });
});
