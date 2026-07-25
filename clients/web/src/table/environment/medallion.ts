/**
 * The environment's medallion mark (`docs/design/environment-system.md` §2.3,
 * §10.3) — issue #530.
 *
 * Its own module because it is data, not a component: keeping it beside the
 * layer components would make `EnvironmentLayers.tsx` a mixed export and break
 * fast refresh.
 */
import type { GlyphDef } from '../../chrome/glyphs/geometry';

/**
 * The medallion mark, authored in the shared glyph geometry model
 * (`chrome/glyphs/geometry.ts`) rather than as a path string, exactly as §10.3
 * asks. It is defined here instead of being added to `GLYPHS` because the glyph
 * *vocabulary* is a chrome contract with its own catalog-coverage test; the
 * *model* — the `0 0 24 24` box and the three primitives both renderers can draw
 * — is what this reuses.
 *
 * An eight-point rune star inside a ring: rotationally symmetric, so it reads
 * the same from every seat around the arc (§3.2) and carries no orientation a
 * player could mistake for information.
 */
export const ENV_MEDALLION_GLYPH: GlyphDef = {
  title: 'Rune medallion',
  strokeWidth: 1.4,
  elements: [
    { kind: 'circle', cx: 12, cy: 12, r: 10.5 },
    { kind: 'circle', cx: 12, cy: 12, r: 7.5 },
    {
      kind: 'polygon',
      points: [
        [12, 3],
        [14, 10],
        [21, 12],
        [14, 14],
        [12, 21],
        [10, 14],
        [3, 12],
        [10, 10],
      ],
    },
    { kind: 'circle', cx: 12, cy: 12, r: 2.2, fill: true },
  ],
};
