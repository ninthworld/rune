import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { DeckBuilder } from './DeckBuilder';
import { STARTER_DECKLISTS, decklistCounts, decklistSize } from './decklists';
import { CATALOG_VIEW } from './catalog-view.fixture';

afterEach(cleanup);

/** Render the builder over the fixture catalog with sensible defaults. */
function renderBuilder(overrides: Partial<Parameters<typeof DeckBuilder>[0]> = {}): {
  onSubmit: ReturnType<typeof vi.fn>;
  onClose: ReturnType<typeof vi.fn>;
} {
  const onSubmit = vi.fn();
  const onClose = vi.fn();
  render(
    <DeckBuilder
      catalog={CATALOG_VIEW}
      format={CATALOG_VIEW.formats[0]}
      initialCounts={{}}
      onSubmit={onSubmit}
      onClose={onClose}
      {...overrides}
    />,
  );
  return { onSubmit, onClose };
}

describe('DeckBuilder (issue #368)', () => {
  it('lists every supported card from the catalog', () => {
    renderBuilder();
    for (const card of CATALOG_VIEW.cards) {
      expect(screen.getByTestId(`deck-builder-card-${card.functional_id}`)).toBeDefined();
    }
  });

  it('shows each card’s cost and type line for browsing', () => {
    renderBuilder();
    const angel = screen.getByTestId('deck-builder-card-serra_angel');
    expect(angel.textContent).toContain('{3}{W}{W}');
    expect(angel.textContent).toContain('Creature — Angel');
  });

  it('inspects a card with its rules text through the shared inspect treatment', () => {
    renderBuilder();
    fireEvent.click(screen.getByTestId('deck-builder-inspect-serra_angel'));
    // The universal CardInspect popover renders the server-computed rules text verbatim.
    expect(screen.getByTestId('card-inspect')).toBeDefined();
    expect(screen.getByTestId('card-inspect-name').textContent).toBe('Serra Angel');
    expect(screen.getByTestId('card-inspect-rules').textContent).toContain('Flying, vigilance');
  });

  it('adds and removes copies, tracking per-card and running counts', () => {
    renderBuilder();
    expect(screen.getByTestId('deck-builder-total').textContent).toBe('0 cards');
    expect(screen.getByTestId('deck-builder-count-shock').textContent).toBe('0');

    // Two Shocks and one Angel.
    fireEvent.click(screen.getByTestId('deck-builder-add-shock'));
    fireEvent.click(screen.getByTestId('deck-builder-add-shock'));
    fireEvent.click(screen.getByTestId('deck-builder-add-serra_angel'));
    expect(screen.getByTestId('deck-builder-count-shock').textContent).toBe('2');
    expect(screen.getByTestId('deck-builder-total').textContent).toBe('3 cards');

    // Remove one Shock: the per-card and running counts both drop.
    fireEvent.click(screen.getByTestId('deck-builder-remove-shock'));
    expect(screen.getByTestId('deck-builder-count-shock').textContent).toBe('1');
    expect(screen.getByTestId('deck-builder-total').textContent).toBe('2 cards');
  });

  it('cannot remove below zero (the remove control disables at zero)', () => {
    renderBuilder();
    const remove = screen.getByTestId('deck-builder-remove-shock') as HTMLButtonElement;
    expect(remove.disabled).toBe(true);
  });

  it('submits the built list as functional ids with duplicates repeated', () => {
    const { onSubmit } = renderBuilder();
    fireEvent.click(screen.getByTestId('deck-builder-add-shock'));
    fireEvent.click(screen.getByTestId('deck-builder-add-shock'));
    fireEvent.click(screen.getByTestId('deck-builder-add-serra_angel'));

    fireEvent.click(screen.getByTestId('deck-builder-submit'));
    expect(onSubmit).toHaveBeenCalledTimes(1);
    const cards = onSubmit.mock.calls[0][0] as string[];
    expect(cards).toHaveLength(3);
    expect(cards.filter((c) => c === 'shock')).toHaveLength(2);
    expect(cards.filter((c) => c === 'serra_angel')).toHaveLength(1);
  });

  it('displays the format’s advertised deck rules as information (no legality here)', () => {
    renderBuilder();
    const rules = screen.getByTestId('deck-builder-format');
    // The strict 1v1 format: min 40, four copies with basics exempt, two players.
    expect(rules.textContent).toContain('Minimum 40 cards');
    expect(rules.textContent).toContain('Up to 4 copies');
    expect(rules.textContent).toContain('basic lands exempt');
    expect(rules.textContent).toContain('2 players');
  });

  it('reads a permissive format’s absent bounds as “no limit” honestly', () => {
    renderBuilder({ format: CATALOG_VIEW.formats[1] });
    const rules = screen.getByTestId('deck-builder-format');
    expect(rules.textContent).toContain('No minimum deck size');
    expect(rules.textContent).toContain('No copy limit');
    expect(rules.textContent).toContain('2–8 players');
  });

  it('seeds from a starter deck as a starting point for editing', () => {
    const starter = STARTER_DECKLISTS[0];
    renderBuilder({ initialCounts: decklistCounts(starter) });
    expect(screen.getByTestId('deck-builder-total').textContent).toBe(
      `${decklistSize(starter)} cards`,
    );
  });

  it('loads a starter into the builder with one tap, then lets it be edited', () => {
    const starter = STARTER_DECKLISTS[0];
    const { onSubmit } = renderBuilder();
    fireEvent.click(screen.getByTestId(`deck-builder-starter-${starter.id}`));
    expect(screen.getByTestId('deck-builder-total').textContent).toBe(
      `${decklistSize(starter)} cards`,
    );

    fireEvent.click(screen.getByTestId('deck-builder-submit'));
    expect((onSubmit.mock.calls[0][0] as string[]).length).toBe(decklistSize(starter));
  });

  it('shows a loading state until the catalog arrives, without a dead screen', () => {
    const { onClose } = renderBuilder({ catalog: null });
    expect(screen.getByTestId('deck-builder-loading')).toBeDefined();
    // The pool is absent, but the modal stays interactive (Close/Cancel present).
    expect(screen.queryByTestId('deck-builder-pool')).toBeNull();
    fireEvent.click(screen.getByTestId('deck-builder-close'));
    expect(onClose).toHaveBeenCalled();
  });

  it('surfaces a rejection over the modal while preserving the built list', () => {
    // A rejection arrives as the lobby's non-fatal error; the builder keeps its state.
    render(
      <DeckBuilder
        catalog={CATALOG_VIEW}
        format={CATALOG_VIEW.formats[0]}
        initialCounts={{ shock: 3 }}
        onSubmit={vi.fn()}
        onClose={vi.fn()}
        error="That deck was rejected. Pick a deck and submit again."
      />,
    );
    expect(screen.getByTestId('deck-builder-error').textContent).toContain('rejected');
    // State preserved: the three Shocks are still in the deck for correction.
    expect(screen.getByTestId('deck-builder-count-shock').textContent).toBe('3');
    expect(screen.getByTestId('deck-builder-total').textContent).toBe('3 cards');
  });

  it('closes on Escape, backdrop, and Cancel (full keyboard + pointer operability)', () => {
    const onClose = vi.fn();
    render(
      <DeckBuilder
        catalog={CATALOG_VIEW}
        format={CATALOG_VIEW.formats[0]}
        initialCounts={{}}
        onSubmit={vi.fn()}
        onClose={onClose}
      />,
    );
    fireEvent.keyDown(screen.getByTestId('deck-builder'), { key: 'Escape' });
    fireEvent.click(screen.getByTestId('deck-builder-cancel'));
    fireEvent.click(screen.getByTestId('deck-builder-backdrop'));
    expect(onClose).toHaveBeenCalledTimes(3);
  });
});
