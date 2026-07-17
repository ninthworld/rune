/**
 * DOM renderer for the procedural glyph language (issue #317, ADR 0003 §DOM side).
 *
 * Turns a shared {@link GlyphDef} (from `geometry.ts`) into inline SVG — the same
 * mould as {@link RuneMark}. Nothing here defines geometry: the primitives come from
 * the one authored source, so the Pixi drawer (`pixi.ts`) and this component always
 * agree. Color rides `currentColor`, so a caller tints a glyph purely by setting the
 * surrounding `color` (typically a chrome token), never a hard-coded hex.
 */
import {
  DEFAULT_STROKE,
  GLYPHS,
  GLYPH_VIEWBOX,
  type GlyphElement,
  type GlyphName,
} from './geometry';

/** Props for {@link Glyph}. */
export interface GlyphProps {
  /** Which glyph to draw. */
  readonly name: GlyphName;
  /** Rendered edge length in CSS px. Defaults to 16 (chip scale). */
  readonly size?: number;
  /**
   * An accessible name. When the glyph stands in for a word (a keyword badge, a zone
   * label), pass the word so assistive tech announces it. Omit for a purely
   * decorative mark, and the SVG is `aria-hidden` — nothing announced twice.
   */
  readonly label?: string;
  /** Optional class on the `<svg>` (e.g. to set its `color`). */
  readonly className?: string;
}

/** One glyph primitive as an SVG element. `currentColor` supplies stroke and fill. */
function renderElement(el: GlyphElement, key: number) {
  const stroke = 'currentColor';
  const common = {
    stroke,
    fill: 'fill' in el && el.fill ? 'currentColor' : 'none',
    strokeLinecap: 'round' as const,
    strokeLinejoin: 'round' as const,
  };
  switch (el.kind) {
    case 'circle':
      return <circle key={key} cx={el.cx} cy={el.cy} r={el.r} {...common} />;
    case 'polygon':
      return <polygon key={key} points={el.points.map((p) => p.join(',')).join(' ')} {...common} />;
    case 'polyline':
      return (
        <polyline
          key={key}
          points={el.points.map((p) => p.join(',')).join(' ')}
          {...common}
          fill="none"
        />
      );
  }
}

/**
 * Render one glyph as inline SVG scaled to `size`. Geometry is drawn in the shared
 * `0 0 24 24` box and scaled by the `viewBox`, so the stroke thickens/thins with the
 * mark and stays proportionate at every tier from a chip to a HUD marker.
 */
export function Glyph({ name, size = 16, label, className }: GlyphProps) {
  const def = GLYPHS[name];
  const strokeWidth = def.strokeWidth ?? DEFAULT_STROKE;
  const accessible = label !== undefined;
  return (
    <svg
      className={className}
      width={size}
      height={size}
      viewBox={`0 0 ${GLYPH_VIEWBOX} ${GLYPH_VIEWBOX}`}
      fill="none"
      strokeWidth={strokeWidth}
      role={accessible ? 'img' : undefined}
      aria-label={accessible ? label : undefined}
      aria-hidden={accessible ? undefined : true}
      focusable="false"
      data-glyph={name}
    >
      {accessible ? <title>{label}</title> : null}
      {def.elements.map((el, i) => renderElement(el, i))}
    </svg>
  );
}
