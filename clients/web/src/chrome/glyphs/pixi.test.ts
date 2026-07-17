/**
 * Pixi gallery for the glyph language (issue #317).
 *
 * Proves every glyph renders in the canvas context from the SAME authored source as
 * the DOM component — the single-source acceptance criterion. Runs headless: a
 * `Graphics` records geometry without any GPU/GL (see `src/test/setup.ts`).
 */
import { Container, Graphics } from 'pixi.js';
import { describe, expect, it } from 'vitest';
import { buildGlyphDisplay } from './pixi';
import { GLYPH_NAMES, GLYPH_VIEWBOX } from './geometry';

describe('buildGlyphDisplay (Pixi)', () => {
  it('draws every glyph as scaled canvas geometry', () => {
    for (const name of GLYPH_NAMES) {
      const display = buildGlyphDisplay(name, { size: 16, color: '#E8E6E1' });
      expect(display).toBeInstanceOf(Container);
      const g = display.getChildAt(0) as Graphics;
      expect(g).toBeInstanceOf(Graphics);
      // Recorded at least one primitive (line/fill) — no empty glyph slipped in.
      expect(g.geometry.graphicsData.length, `${name} drew nothing`).toBeGreaterThan(0);
    }
  });

  it('scales the container from the 24-unit box to the requested px size', () => {
    const display = buildGlyphDisplay('tap', { size: 48, color: 0xffffff });
    expect(display.scale.x).toBeCloseTo(48 / GLYPH_VIEWBOX);
    expect(display.scale.y).toBeCloseTo(48 / GLYPH_VIEWBOX);
  });

  it('accepts a token color string and a raw number alike', () => {
    expect(() => buildGlyphDisplay('kw-flying', { color: '#F2C94C' })).not.toThrow();
    expect(() => buildGlyphDisplay('kw-flying', { color: 0xf2c94c })).not.toThrow();
  });
});
