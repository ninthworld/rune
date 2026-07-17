/**
 * Pixi renderer for the procedural glyph language (issue #317, ADR 0003 §canvas).
 *
 * Draws a shared {@link GlyphDef} into a Pixi `Graphics` from the SAME authored
 * primitives the DOM `<Glyph>` component uses — the single-source rule of issue #317.
 * Unlike DOM's `currentColor`, the canvas has no inherited color, so the tint is an
 * explicit argument the caller derives from a token (never a hard-coded literal).
 *
 * Geometry is authored in the `0 0 24 24` box and the returned container is scaled to
 * the requested pixel size, so one definition serves every tier (a 12-px chip tap
 * glyph and a larger HUD marker are the same geometry at different scale).
 */
import { Container, Graphics, LINE_CAP, LINE_JOIN } from 'pixi.js';
import { DEFAULT_STROKE, GLYPHS, GLYPH_VIEWBOX, type GlyphName } from './geometry';

/** `'#RRGGBB'` (or a raw number) to the numeric color Pixi expects. */
function toColorNumber(color: string | number): number {
  return typeof color === 'number' ? color : parseInt(color.replace('#', ''), 16);
}

/** Options for {@link buildGlyphDisplay}. */
export interface GlyphDrawOptions {
  /** Rendered edge length in px. Defaults to 16 (chip scale). */
  readonly size?: number;
  /** Tint for strokes and fills — a token color string (`'#RRGGBB'`) or a number. */
  readonly color: string | number;
  /** Optional stroke alpha (e.g. a dimmed marker). Defaults to 1. */
  readonly alpha?: number;
}

/**
 * Build a Pixi display object for one glyph, scaled to `size` and tinted to `color`.
 * The geometry is drawn in the 24-unit box on a child `Graphics`, and the returned
 * `Container` is scaled to the pixel size — so callers position the container without
 * re-deriving layout. Stroke width is authored in glyph units, so it scales with the
 * mark exactly as the DOM `strokeWidth`/`viewBox` pairing does.
 */
export function buildGlyphDisplay(name: GlyphName, opts: GlyphDrawOptions): Container {
  const def = GLYPHS[name];
  const color = toColorNumber(opts.color);
  const alpha = opts.alpha ?? 1;
  const width = def.strokeWidth ?? DEFAULT_STROKE;

  const g = new Graphics();
  const stroke = { width, color, alpha, cap: LINE_CAP.ROUND, join: LINE_JOIN.ROUND };

  for (const el of def.elements) {
    switch (el.kind) {
      case 'polyline': {
        g.lineStyle(stroke);
        el.points.forEach(([x, y], i) => (i === 0 ? g.moveTo(x, y) : g.lineTo(x, y)));
        break;
      }
      case 'polygon': {
        const filled = 'fill' in el && el.fill === true;
        g.lineStyle(stroke);
        if (filled) g.beginFill(color, alpha);
        g.drawPolygon(el.points.flatMap(([x, y]) => [x, y]));
        if (filled) g.endFill();
        break;
      }
      case 'circle': {
        const filled = 'fill' in el && el.fill === true;
        g.lineStyle(stroke);
        if (filled) g.beginFill(color, alpha);
        g.drawCircle(el.cx, el.cy, el.r);
        if (filled) g.endFill();
        break;
      }
    }
  }
  // Flush a trailing open polyline into the geometry. Pixi defers this to render
  // time, but flushing now keeps the display object's geometry complete for headless
  // consumers (and the gallery test) that inspect it before any frame is drawn.
  g.finishPoly();

  const container = new Container();
  container.addChild(g);
  const size = opts.size ?? 16;
  container.scale.set(size / GLYPH_VIEWBOX);
  return container;
}
