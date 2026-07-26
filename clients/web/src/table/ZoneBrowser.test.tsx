import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import type { CardView } from '../protocol';
import { ZoneBrowser } from './ZoneBrowser';
import { ZONE_BROWSER } from './zoneBrowserView';

afterEach(cleanup);

const CARDS: CardView[] = [
  {
    id: 'g1',
    name: 'Llanowar Elves',
    type_line: 'Creature — Elf Druid',
    power: '1',
    toughness: '1',
  },
  { id: 'g2', name: 'Giant Growth', type_line: 'Instant', mana_cost: '{G}' },
  { id: 'g3', name: 'Forest', type_line: 'Basic Land — Forest' },
];

/** A pile of `n` distinguishable cards in wire order (index 0 is the bottom). */
function pile(n: number): CardView[] {
  return Array.from({ length: n }, (_, i) => ({
    id: `c${i}`,
    name: `Card ${i}`,
    type_line: 'Instant',
  }));
}

function renderBrowser(props: Partial<Parameters<typeof ZoneBrowser>[0]> = {}) {
  return render(
    <ZoneBrowser
      zone="graveyard"
      owner="p1"
      cards={CARDS}
      onInspect={vi.fn()}
      onClose={vi.fn()}
      {...props}
    />,
  );
}

describe('ZoneBrowser (issues #262, #584)', () => {
  it('draws real card faces with the one card renderer, not text rows', () => {
    renderBrowser();
    const browser = screen.getByTestId('zone-browser');
    // The face component's own root: a presentational card named for the card.
    const faces = within(browser).getAllByRole('img', {
      name: /Llanowar Elves|Giant Growth|Forest/,
    });
    expect(faces).toHaveLength(3);
    // …and it is really the shared frame: the tier, the silhouette, and the
    // frame's own bands are all there, none of which a text row has.
    const elves = within(screen.getByTestId('browser-card-g1')).getByRole('img', {
      name: 'Llanowar Elves',
    });
    expect(elves.getAttribute('data-tier')).toBe('stack');
    expect(elves.getAttribute('data-kind')).toBe('card');
    expect(elves.textContent).toContain('Creature — Elf Druid');
    expect(elves.textContent).toContain('1/1');
  });

  it('presents the pile top-first and says so in the heading', () => {
    renderBrowser();
    expect(screen.getByTestId('zone-browser-title').textContent).toBe('p1 — Graveyard');
    expect(screen.getByTestId('zone-browser-order').textContent).toBe(
      '3 cards · top of pile first',
    );

    // The wire order is top-last, so the top card leads the grid…
    const cards = within(screen.getByTestId('zone-browser-grid')).getAllByRole('listitem');
    expect(cards[0]!.textContent).toContain('Forest');
    expect(cards[2]!.textContent).toContain('Llanowar Elves');
    // …and every entry still carries the server's own index, so nothing is lost.
    expect(screen.getByTestId('browser-card-g3').getAttribute('data-pile-index')).toBe('2');
    expect(screen.getByTestId('browser-card-g1').getAttribute('data-pile-index')).toBe('0');
    expect(screen.getByTestId('browser-card-g3').getAttribute('aria-label')).toBe(
      'Inspect Forest, top of pile',
    );
  });

  it('opens inspect for a card and reports its id', () => {
    const onInspect = vi.fn();
    renderBrowser({ onInspect });
    fireEvent.click(screen.getByTestId('browser-card-g2'));
    expect(onInspect).toHaveBeenCalledWith('g2');
  });

  it('reads exile as a distinct zone, in the surface and in words', () => {
    renderBrowser({ zone: 'exile', cards: [CARDS[0]!] });
    const browser = screen.getByTestId('zone-browser');
    // Not colour alone (`visual-system.md` §7): the zone is named, and the panel
    // carries the attribute its own identity treatment selects on.
    expect(browser.getAttribute('data-zone')).toBe('exile');
    expect(screen.getByTestId('zone-browser-title').textContent).toBe('p1 — Exile');
    expect(browser.getAttribute('aria-label')).toContain('exile');
  });

  it('renders a designed quiet state for an empty pile', () => {
    renderBrowser({ zone: 'exile', cards: [] });
    const empty = screen.getByTestId('zone-browser-empty');
    // The zone's own etched glyph inside a card silhouette, not a `No cards.`
    // paragraph. The silhouette is decoration, so it is `aria-hidden` and the
    // spoken empty state is the dialog's own name (asserted below).
    expect(empty.querySelector('svg')).not.toBeNull();
    expect(empty.textContent).toContain('Nothing has been exiled.');
    expect(screen.getByTestId('zone-browser-order').textContent).toBe('empty');
    expect(screen.getByTestId('zone-browser').getAttribute('aria-label')).toBe('p1 exile, empty');
    expect(screen.queryByTestId('zone-browser-grid')).toBeNull();
  });

  it('bounds mounted faces and pages a pile past the cap', () => {
    const big = pile(ZONE_BROWSER.block + 5);
    renderBrowser({ cards: big });
    const grid = screen.getByTestId('zone-browser-grid');
    expect(within(grid).getAllByRole('listitem')).toHaveLength(ZONE_BROWSER.block);
    expect(screen.getByTestId('zone-browser-block').textContent).toBe('Block 1 of 2');
    expect(screen.getByTestId('zone-browser-prev')).toHaveProperty('disabled', true);

    fireEvent.click(screen.getByTestId('zone-browser-next'));
    expect(screen.getByTestId('zone-browser-block').textContent).toBe('Block 2 of 2');
    expect(within(screen.getByTestId('zone-browser-grid')).getAllByRole('listitem')).toHaveLength(
      5,
    );
    // The second block continues top-first: it ends on the bottom of the pile.
    expect(screen.getByTestId('browser-card-c0').getAttribute('data-pile-index')).toBe('0');
  });

  it('shows no block controls for a pile that fits', () => {
    renderBrowser();
    expect(screen.queryByTestId('zone-browser-pager')).toBeNull();
  });

  it('traverses the grid with the arrow keys as well as with Tab', () => {
    renderBrowser();
    const grid = screen.getByTestId('zone-browser-grid');
    const top = screen.getByTestId('browser-card-g3');
    top.focus();

    fireEvent.keyDown(grid, { key: 'ArrowRight' });
    expect(document.activeElement).toBe(screen.getByTestId('browser-card-g2'));
    fireEvent.keyDown(grid, { key: 'ArrowDown' });
    expect(document.activeElement).toBe(screen.getByTestId('browser-card-g1'));
    fireEvent.keyDown(grid, { key: 'ArrowLeft' });
    expect(document.activeElement).toBe(screen.getByTestId('browser-card-g2'));
    fireEvent.keyDown(grid, { key: 'Home' });
    expect(document.activeElement).toBe(top);
    fireEvent.keyDown(grid, { key: 'End' });
    expect(document.activeElement).toBe(screen.getByTestId('browser-card-g1'));
  });

  it('closes on the close control and on a backdrop click', () => {
    const onClose = vi.fn();
    renderBrowser({ cards: [], onClose });
    fireEvent.click(screen.getByTestId('zone-browser-close'));
    fireEvent.click(screen.getByTestId('zone-browser-backdrop'));
    expect(onClose).toHaveBeenCalledTimes(2);
  });

  it('does not close when the panel itself is clicked', () => {
    const onClose = vi.fn();
    renderBrowser({ onClose });
    fireEvent.click(screen.getByTestId('zone-browser'));
    expect(onClose).not.toHaveBeenCalled();
  });
});
