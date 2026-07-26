/**
 * The mana reservoir (issue #567, absorbing #462's remaining scope).
 *
 * Floating mana had no home on the 2.5D surface at all: the only component that
 * ever drew `view.mana_pool` was `MePanel`, an ADR 0023 survivor nothing
 * imported. These pin the four things the issue asks for — symbolised, named for
 * a reader, literal for an unknown code, and presentation only.
 */
import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import { ManaReservoir } from './ManaReservoir';

afterEach(cleanup);

/** The accessible names of every symbol drawn inside an element, in order. */
function symbols(host: HTMLElement): (string | null)[] {
  return Array.from(host.querySelectorAll('[data-symbol]')).map((el) =>
    el.getAttribute('aria-label'),
  );
}

describe('ManaReservoir', () => {
  it('draws the pool through the shared symbol renderer', () => {
    render(<ManaReservoir pool={['{G}', '{G}', '{U}']} />);
    const reservoir = screen.getByTestId('mana-reservoir');

    // No brace notation reaches the player at any surface (#462).
    expect(reservoir.textContent).not.toContain('{');
    expect(symbols(reservoir)).toEqual(['green mana', 'green mana', 'blue mana']);
  });

  it('keeps the server’s own order and multiplicity', () => {
    // The client never sums or normalises a pool — two greens are two discs.
    render(<ManaReservoir pool={['{U}', '{G}', '{U}']} />);
    expect(symbols(screen.getByTestId('mana-reservoir'))).toEqual([
      'blue mana',
      'green mana',
      'blue mana',
    ]);
  });

  it('names the whole pool for a reader, because each pip is role="img"', () => {
    render(<ManaReservoir pool={['{G}', '{1}']} />);
    expect(
      screen.getByRole('group', { name: 'Mana pool: green mana, one generic mana' }),
    ).toBeTruthy();
  });

  it('prints an unknown code literally rather than dropping it', () => {
    // The vocabulary is `chrome/symbols`'s; a code it does not know renders as
    // its own text, so a future server symbol degrades instead of vanishing.
    render(<ManaReservoir pool={['{G}', '{PHYREXIAN}']} />);
    expect(screen.getByTestId('mana-reservoir').textContent).toContain('{PHYREXIAN}');
  });

  it('is absent when the pool is empty', () => {
    // ADR 0032: contextual chrome appears when relevant and is otherwise
    // absent. An empty reservoir is a permanent widget reading zero — the
    // always-there dashboard the ADR removed. "Always visible WHEN PRESENT".
    const { container } = render(<ManaReservoir pool={[]} />);
    expect(container.firstChild).toBeNull();
    expect(screen.queryByTestId('mana-reservoir')).toBeNull();
  });

  it('offers nothing to press: mana is state, not an action', () => {
    // Spending stays server-authoritative — a mana ability is a `ValidAction` on
    // the land, not a control here. Nothing in this row is focusable.
    render(<ManaReservoir pool={['{G}']} />);
    const reservoir = screen.getByTestId('mana-reservoir');
    expect(reservoir.querySelectorAll('button, a, input, [tabindex]')).toHaveLength(0);
  });
});
