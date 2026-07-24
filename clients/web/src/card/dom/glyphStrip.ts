import {
  DEFAULT_STROKE,
  GLYPHS,
  GLYPH_VIEWBOX,
  type GlyphElement,
  type GlyphName,
} from '../../chrome/glyphs';

/**
 * A keyword strip serialized into at most two SVG path strings, so the whole
 * strip renders as one `<svg>` with one stroked `<path>` (plus one filled
 * `<path>` only when a glyph uses filled primitives) — the DOM-budget shape of
 * the card face (presentation-budgets §Performance: ≤ 12 nodes per
 * battlefield-tier face). Geometry comes only from the shared glyph source
 * (`chrome/glyphs/geometry.ts`), so the Pixi drawer and this strip always agree.
 */
export interface GlyphStripGeometry {
  /** Path data stroked with `currentColor` — every primitive of every glyph. */
  stroke: string;
  /** Path data filled with `currentColor` — only primitives authored `fill: true`. */
  fill: string;
  /** The strip's viewBox width in glyph units (height is one glyph box). */
  width: number;
  /** The shared stroke width, in glyph units. */
  strokeWidth: number;
}

/** Horizontal gap between glyph boxes, in glyph units. */
const GLYPH_GAP = 4;

/** Serialize one primitive at a horizontal offset into path data. */
function elementPath(el: GlyphElement, dx: number): string {
  if (el.kind === 'circle') {
    const { cx, cy, r } = el;
    const x = cx + dx;
    // Two half-circle arcs draw the full ring.
    return `M ${x - r} ${cy} a ${r} ${r} 0 1 0 ${2 * r} 0 a ${r} ${r} 0 1 0 ${-2 * r} 0`;
  }
  const points = el.points.map(([x, y], i) => `${i === 0 ? 'M' : 'L'} ${x + dx} ${y}`).join(' ');
  return el.kind === 'polygon' ? `${points} Z` : points;
}

/**
 * Serialize a row of glyphs into the two-path strip geometry. `names` should
 * already be capped to the tier's capacity by the caller (the face degrades the
 * overflow to a `+N` tag, carried from the shipped strip).
 */
export function glyphStripGeometry(names: GlyphName[]): GlyphStripGeometry {
  const stroke: string[] = [];
  const fill: string[] = [];
  let strokeWidth = DEFAULT_STROKE;
  names.forEach((name, i) => {
    const def = GLYPHS[name];
    strokeWidth = Math.max(strokeWidth, def.strokeWidth ?? DEFAULT_STROKE);
    const dx = i * (GLYPH_VIEWBOX + GLYPH_GAP);
    for (const el of def.elements) {
      const d = elementPath(el, dx);
      stroke.push(d);
      if ('fill' in el && el.fill) fill.push(d);
    }
  });
  const width =
    names.length === 0 ? GLYPH_VIEWBOX : names.length * (GLYPH_VIEWBOX + GLYPH_GAP) - GLYPH_GAP;
  return { stroke: stroke.join(' '), fill: fill.join(' '), width, strokeWidth };
}
