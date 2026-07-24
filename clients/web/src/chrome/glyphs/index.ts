/**
 * The RUNE procedural glyph language (issue #317). One authored geometry source
 * (`geometry.ts`) rendered by the DOM `<Glyph>` component. The former Pixi
 * `buildGlyphDisplay` drawer (ADR 0003) was retired with the legacy scene stack
 * (issue #504). See `docs/design/ui-design-notes.md` (§Identity, §Card render).
 */
export { Glyph, type GlyphProps } from './Glyph';
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
