/**
 * Zone piles as findable spatial objects (issue #319).
 *
 * Covers the pile as a first-class object: its count home, the browser-opening
 * affordance for graveyard/exile across input modes, the count-only library, empty
 * piles staying visible, and the pile frame's ability to host a face-up card.
 */
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { ZonePile } from './ZonePile';

afterEach(cleanup);

describe('ZonePile (issue #319)', () => {
  it('renders the library as a count-only, non-interactive pile (no contents)', () => {
    render(<ZonePile zone="library" playerLabel="p1 (you)" count={37} testId="lib" />);
    const pile = screen.getByTestId('lib');
    // Not a button — the library never offers its contents.
    expect(pile.tagName).not.toBe('BUTTON');
    expect(pile.getAttribute('role')).toBe('img');
    expect(pile.getAttribute('aria-label')).toBe('p1 (you) library (37)');
    expect(pile.textContent).toContain('37');
  });

  it('opens a graveyard/exile browser via a focusable button (pointer/touch/keyboard)', () => {
    const onOpen = vi.fn();
    render(<ZonePile zone="graveyard" playerLabel="p2" count={3} onOpen={onOpen} testId="gy" />);
    const pile = screen.getByTestId('gy');
    // A real <button>: keyboard- and controller-focusable, and a ≥44px touch target.
    expect(pile.tagName).toBe('BUTTON');
    expect(pile.getAttribute('aria-label')).toBe('Browse p2 graveyard (3)');
    fireEvent.click(pile);
    expect(onOpen).toHaveBeenCalledTimes(1);
  });

  it('keeps an empty zone visible as an empty pile, not hidden', () => {
    render(
      <ZonePile zone="exile" playerLabel="p1 (you)" count={0} onOpen={() => {}} testId="ex" />,
    );
    const pile = screen.getByTestId('ex');
    expect(pile).toBeDefined();
    expect(pile.textContent).toContain('0');
  });

  it('identifies each pile by its zone glyph', () => {
    const { container } = render(
      <ZonePile zone="graveyard" playerLabel="p1" count={2} onOpen={() => {}} />,
    );
    expect(container.querySelector('svg[data-glyph="zone-graveyard"]')).not.toBeNull();
  });

  it('renders the command zone as its own labelled pile with the crown glyph (issue #372)', () => {
    const { container } = render(
      <ZonePile zone="command" playerLabel="p1 (you)" count={1} testId="cmd" />,
    );
    const pile = screen.getByTestId('cmd');
    // The command zone is a static pile (no browser), named for assistive tech.
    expect(pile.tagName).not.toBe('BUTTON');
    expect(pile.getAttribute('aria-label')).toBe('p1 (you) command (1)');
    expect(pile.textContent).toContain('command');
    expect(container.querySelector('svg[data-glyph="zone-command"]')).not.toBeNull();
  });

  it('hosts a face-up card in the pile frame without layout change (future reveal)', () => {
    render(
      <ZonePile
        zone="library"
        playerLabel="p1 (you)"
        count={40}
        faceUp={<span data-testid="revealed">Top card</span>}
        testId="lib"
      />,
    );
    // The revealed card renders inside the pile; the glyph gives way to it.
    expect(screen.getByTestId('revealed')).toBeDefined();
    expect(screen.getByTestId('lib').querySelector('svg[data-glyph="zone-library"]')).toBeNull();
  });
});
