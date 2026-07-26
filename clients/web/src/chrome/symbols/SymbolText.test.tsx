/**
 * Inline symbol rendering (issue #462). jsdom applies no CSS module, so nothing
 * here claims a drawn size: what is asserted is the contract — which nodes are
 * produced, what each announces, and that no brace notation survives into the
 * text a player reads.
 */
import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import { PIP } from '../../tokens';
import { SymbolText } from './SymbolText';

afterEach(cleanup);

/** Render a string and return the host element. */
function draw(text: string): HTMLElement {
  const { container } = render(
    <p data-testid="host">
      <SymbolText text={text} />
    </p>,
  );
  return container.querySelector('[data-testid="host"]') as HTMLElement;
}

/** The accessible names of the drawn symbols, in order. */
function names(host: HTMLElement): (string | null)[] {
  return Array.from(host.querySelectorAll('[data-symbol]')).map((el) =>
    el.getAttribute('aria-label'),
  );
}

describe('SymbolText', () => {
  it('draws each symbol as an announced icon and keeps the surrounding words', () => {
    const host = draw('{T}: Add {G}.');
    expect(host.textContent).not.toContain('{');
    expect(host.textContent).toContain(': Add ');
    expect(names(host)).toEqual(['tap', 'green mana']);
  });

  it('renders the tap symbol as the procedural glyph, not a letter', () => {
    const host = draw('{T}');
    const symbol = host.querySelector('[data-symbol="T"]')!;
    expect(symbol.querySelector('svg')?.getAttribute('data-glyph')).toBe('tap');
    expect(symbol.textContent).toBe('');
  });

  it('colors a pip from the shipped PIP tokens — no literal in the stylesheet', () => {
    const host = draw('{G}');
    const symbol = host.querySelector<HTMLElement>('[data-symbol="G"]')!;
    expect(symbol.style.getPropertyValue('--symbol-bg')).toBe(PIP.G.bg);
    expect(symbol.style.getPropertyValue('--symbol-fg')).toBe(PIP.G.fg);
  });

  it('gives a multi-part code the wide plate — a non-color channel of its own', () => {
    const host = draw('{W/U}{G}');
    expect(host.querySelector('[data-symbol="W/U"]')?.getAttribute('data-wide')).toBe('true');
    expect(host.querySelector('[data-symbol="G"]')?.getAttribute('data-wide')).toBeNull();
  });

  it('degrades an unrecognized code to visible literal text, never to nothing', () => {
    const host = draw('Pay {WEIRD} now');
    expect(host.textContent).toBe('Pay {WEIRD} now');
    expect(names(host)).toEqual([]);
  });

  it('announces the symbols instead of their letters (AC 2)', () => {
    draw('{2}{W/U}');
    expect(screen.getByLabelText('two generic mana')).toBeTruthy();
    expect(screen.getByLabelText('white or blue mana')).toBeTruthy();
  });
});
