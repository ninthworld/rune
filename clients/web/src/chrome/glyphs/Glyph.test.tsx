/**
 * DOM gallery for the glyph language (issue #317).
 *
 * Enumerates every glyph at chip scale so additions stay reviewed and legible, and
 * pins the accessibility contract: a glyph standing in for a word carries that word
 * as its accessible name; a decorative glyph is hidden from assistive tech.
 */
import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import { Glyph } from './Glyph';
import { GLYPHS, GLYPH_NAMES } from './geometry';

afterEach(cleanup);

describe('Glyph (DOM)', () => {
  it('renders the entire vocabulary at chip scale (~14px)', () => {
    const { container } = render(
      <div>
        {GLYPH_NAMES.map((name) => (
          <Glyph key={name} name={name} size={14} label={GLYPHS[name].title} />
        ))}
      </div>,
    );
    const svgs = container.querySelectorAll('svg[data-glyph]');
    expect(svgs.length).toBe(GLYPH_NAMES.length);
    // Every glyph actually drew primitives at that size.
    svgs.forEach((svg) => {
      expect(svg.getAttribute('width')).toBe('14');
      expect(svg.children.length).toBeGreaterThan(0);
    });
  });

  it('gives a labeled glyph an accessible name (stands in for a word)', () => {
    render(<Glyph name="kw-flying" label="Flying" />);
    const img = screen.getByRole('img', { name: 'Flying' });
    expect(img.getAttribute('data-glyph')).toBe('kw-flying');
  });

  it('hides a decorative glyph from assistive tech', () => {
    const { container } = render(<Glyph name="phase-main" />);
    const svg = container.querySelector('svg')!;
    expect(svg.getAttribute('aria-hidden')).toBe('true');
    expect(svg.getAttribute('role')).toBeNull();
  });

  it('colors from currentColor, never a baked hex', () => {
    const { container } = render(<Glyph name="kw-deathtouch" />);
    const html = container.innerHTML;
    expect(html).toContain('currentColor');
    expect(html).not.toMatch(/#[0-9a-fA-F]{6}/);
  });

  it('scales geometry via the viewBox, not per-size point lists', () => {
    const { container } = render(<Glyph name="kw-lifelink" size={48} />);
    const svg = container.querySelector('svg')!;
    expect(svg.getAttribute('viewBox')).toBe('0 0 24 24');
    expect(svg.getAttribute('width')).toBe('48');
  });
});
