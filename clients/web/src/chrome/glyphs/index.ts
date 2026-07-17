/**
 * The RUNE procedural glyph language (issue #317). One authored geometry source
 * (`geometry.ts`) rendered by two consumers: the DOM `<Glyph>` component and the
 * Pixi `buildGlyphDisplay` drawer (ADR 0003). See `docs/design/ui-design-notes.md`
 * (§Identity, §Card render) for the vocabulary and its intended consumers.
 */
export { Glyph, type GlyphProps } from './Glyph';
export { buildGlyphDisplay, type GlyphDrawOptions } from './pixi';
export {
  GLYPHS,
  GLYPH_NAMES,
  GLYPH_VIEWBOX,
  DEFAULT_STROKE,
  keywordGlyphName,
  type GlyphDef,
  type GlyphElement,
  type GlyphName,
  type GlyphPoint,
} from './geometry';
