/**
 * The RUNE procedural **glyph language** (issue #317) — a single authored source
 * for every place a repeated symbol beats a repeated word: basic-land chips, zone
 * piles, phase-group indicators, keyword badges, tap state, and seat/ready markers.
 *
 * Per the hard rules (`AGENTS.md`) and `docs/design/ui-design-notes.md` (§Identity,
 * §Card render), RUNE's look is **procedural geometry only**: no raster assets, no
 * official frames, and nothing resembling a WotC mana / tap / set symbol. Each glyph
 * here is original geometry in the `RuneMark` idiom — hand-placed primitives inside a
 * fixed `0 0 24 24` box — expressed once as plain data so BOTH renderers (ADR 0003)
 * can draw it from one definition: the React DOM `<Glyph>` component and the Pixi
 * `buildGlyphDisplay` canvas drawer. There is exactly one authored source per glyph;
 * neither renderer owns geometry the other lacks.
 *
 * Colors are never baked in. The DOM component draws with `currentColor`, and the
 * Pixi drawer takes an explicit token color — a glyph is a monochrome mark tinted by
 * its surroundings, exactly like {@link RuneMark}.
 */

/** A point in the shared `0 0 24 24` glyph coordinate box. */
export type GlyphPoint = readonly [number, number];

/** The edge length of the shared glyph coordinate box. All geometry lives in `0..24`. */
export const GLYPH_VIEWBOX = 24;

/**
 * One drawable primitive of a glyph. The set is deliberately tiny — an open stroked
 * polyline, a closed polygon (stroked, optionally filled), and a circle (stroked,
 * optionally filled) — because both a `<path>`/`<polygon>`/`<circle>` SVG element and
 * a Pixi `Graphics` call can render each one exactly, so no renderer needs to parse
 * an SVG `d` string. Every glyph is composed from these three.
 */
export type GlyphElement =
  | {
      /** An open, stroked polyline (round caps/joins) — the workhorse stroke. */
      readonly kind: 'polyline';
      readonly points: readonly GlyphPoint[];
    }
  | {
      /** A closed polygon: stroked outline, optionally filled with the tint color. */
      readonly kind: 'polygon';
      readonly points: readonly GlyphPoint[];
      readonly fill?: boolean;
    }
  | {
      /** A circle: stroked ring, optionally a filled disc. */
      readonly kind: 'circle';
      readonly cx: number;
      readonly cy: number;
      readonly r: number;
      readonly fill?: boolean;
    };

/** A complete glyph: an accessible title plus the primitives that draw it. */
export interface GlyphDef {
  /**
   * A human-readable name. It is the DOM accessible name whenever the glyph stands
   * in for a word (e.g. a "Flying" badge), so a screen reader announces the concept
   * the mark replaces rather than nothing.
   */
  readonly title: string;
  /** Stroke weight in glyph units (out of 24). Defaults to {@link DEFAULT_STROKE}. */
  readonly strokeWidth?: number;
  /** The primitives, drawn back-to-front. */
  readonly elements: readonly GlyphElement[];
}

/** Default stroke weight — chosen so a glyph stays legible down to ~12–16 px. */
export const DEFAULT_STROKE = 2.2;

/**
 * Sample a circular arc into a polyline both renderers can draw identically — used
 * for the tap glyph's rotation sweep. Kept as authored code (run once) rather than a
 * hand-typed point list so the curve stays smooth and the single-source rule holds.
 * Angles are degrees, `0°` at the +x axis, increasing counter-clockwise in math
 * convention (y grows downward in the box, matching both SVG and Pixi).
 */
function arc(
  cx: number,
  cy: number,
  r: number,
  startDeg: number,
  endDeg: number,
  steps = 14,
): GlyphPoint[] {
  const points: GlyphPoint[] = [];
  for (let i = 0; i <= steps; i++) {
    const t = startDeg + ((endDeg - startDeg) * i) / steps;
    const a = (t * Math.PI) / 180;
    points.push([cx + r * Math.cos(a), cy + r * Math.sin(a)]);
  }
  return points;
}

/** The tap glyph's rotation sweep: an open ring with a gap at the top-right. */
const TAP_SWEEP = arc(12, 12, 7, -35, 250);
/** The tap arrowhead, tangent to the sweep's start (its clockwise-leading tip). */
const TAP_HEAD_TIP: GlyphPoint = [
  12 + 7 * Math.cos((-35 * Math.PI) / 180),
  12 + 7 * Math.sin((-35 * Math.PI) / 180),
];

/**
 * Every glyph in the vocabulary, keyed by a stable name. The keys are the contract:
 * consumers reference `GLYPHS[name]`, and {@link GlyphName} is derived from them so a
 * typo cannot compile. Keyword glyphs are named `kw-<wire>` where `<wire>` is the
 * engine's lowercase keyword name (see {@link keywordGlyphName}); the coverage test
 * asserts one exists for every keyword the card catalog ships.
 */
const GLYPH_DEFS = {
  // ── Basic-land types (chip-tier land identity) ──────────────────────────────
  // Original abstract emblems, deliberately unlike the WotC mana symbols.
  'land-plains': {
    title: 'Plains',
    elements: [
      {
        kind: 'polyline',
        points: [
          [12, 5],
          [12, 19],
        ],
      },
      {
        kind: 'polyline',
        points: [
          [12, 9],
          [7, 6],
        ],
      },
      {
        kind: 'polyline',
        points: [
          [12, 9],
          [17, 6],
        ],
      },
      {
        kind: 'polyline',
        points: [
          [12, 13],
          [7, 10],
        ],
      },
      {
        kind: 'polyline',
        points: [
          [12, 13],
          [17, 10],
        ],
      },
    ],
  },
  'land-island': {
    title: 'Island',
    elements: [
      {
        kind: 'polyline',
        points: [
          [4, 10],
          [8, 8],
          [12, 10],
          [16, 8],
          [20, 10],
        ],
      },
      {
        kind: 'polyline',
        points: [
          [4, 15],
          [8, 13],
          [12, 15],
          [16, 13],
          [20, 15],
        ],
      },
    ],
  },
  'land-swamp': {
    title: 'Swamp',
    elements: [
      {
        kind: 'polyline',
        points: [
          [4, 8],
          [20, 8],
        ],
      },
      {
        kind: 'polygon',
        points: [
          [6, 8],
          [18, 8],
          [12, 19],
        ],
        fill: true,
      },
    ],
  },
  'land-mountain': {
    title: 'Mountain',
    elements: [
      {
        kind: 'polygon',
        points: [
          [3, 18],
          [10, 6],
          [17, 18],
        ],
      },
      {
        kind: 'polygon',
        points: [
          [13, 18],
          [18, 10],
          [22, 18],
        ],
      },
    ],
  },
  'land-forest': {
    title: 'Forest',
    elements: [
      {
        kind: 'polygon',
        points: [
          [12, 4],
          [6, 12],
          [18, 12],
        ],
      },
      {
        kind: 'polygon',
        points: [
          [12, 9],
          [6, 17],
          [18, 17],
        ],
      },
      {
        kind: 'polyline',
        points: [
          [12, 17],
          [12, 20],
        ],
      },
    ],
  },

  // ── Zone identities (pile objects) ──────────────────────────────────────────
  'zone-library': {
    title: 'Library',
    elements: [
      {
        kind: 'polygon',
        points: [
          [8, 5],
          [20, 5],
          [20, 17],
          [8, 17],
        ],
      },
      {
        kind: 'polygon',
        points: [
          [4, 8],
          [16, 8],
          [16, 20],
          [4, 20],
        ],
      },
    ],
  },
  'zone-graveyard': {
    title: 'Graveyard',
    elements: [
      {
        kind: 'polygon',
        points: [
          [7, 20],
          [7, 11],
          [9, 8],
          [12, 7],
          [15, 8],
          [17, 11],
          [17, 20],
        ],
      },
      {
        kind: 'polyline',
        points: [
          [4, 20],
          [20, 20],
        ],
      },
      {
        kind: 'polyline',
        points: [
          [12, 11],
          [12, 16],
        ],
      },
      {
        kind: 'polyline',
        points: [
          [9, 13],
          [15, 13],
        ],
      },
    ],
  },
  'zone-exile': {
    title: 'Exile',
    elements: [
      { kind: 'circle', cx: 11, cy: 13, r: 6 },
      {
        kind: 'polyline',
        points: [
          [14, 10],
          [20, 4],
        ],
      },
      {
        kind: 'polyline',
        points: [
          [15, 4],
          [20, 4],
          [20, 9],
        ],
      },
    ],
  },

  // ── Phase groups (turn indicator) ───────────────────────────────────────────
  'phase-beginning': {
    title: 'Beginning',
    elements: [
      {
        kind: 'polyline',
        points: [
          [8, 11],
          [12, 7],
          [16, 11],
        ],
      },
      {
        kind: 'polyline',
        points: [
          [12, 7],
          [12, 19],
        ],
      },
    ],
  },
  'phase-main': {
    title: 'Main phase',
    elements: [
      {
        kind: 'polygon',
        points: [
          [12, 5],
          [19, 12],
          [12, 19],
          [5, 12],
        ],
      },
      { kind: 'circle', cx: 12, cy: 12, r: 2, fill: true },
    ],
  },
  'phase-combat': {
    title: 'Combat',
    elements: [
      {
        kind: 'polyline',
        points: [
          [5, 19],
          [19, 5],
        ],
      },
      {
        kind: 'polyline',
        points: [
          [5, 5],
          [19, 19],
        ],
      },
      {
        kind: 'polyline',
        points: [
          [16, 4],
          [20, 8],
        ],
      },
      {
        kind: 'polyline',
        points: [
          [4, 8],
          [8, 4],
        ],
      },
    ],
  },
  'phase-ending': {
    title: 'Ending',
    elements: [
      {
        kind: 'polyline',
        points: [
          [8, 13],
          [12, 17],
          [16, 13],
        ],
      },
      {
        kind: 'polyline',
        points: [
          [12, 5],
          [12, 17],
        ],
      },
    ],
  },

  // ── Keyword abilities (card faces) — one per engine `Keyword` variant ────────
  'kw-flying': {
    title: 'Flying',
    elements: [
      {
        kind: 'polyline',
        points: [
          [4, 15],
          [8, 11],
          [13, 9],
          [20, 7],
        ],
      },
      {
        kind: 'polyline',
        points: [
          [8, 11],
          [8, 14],
        ],
      },
      {
        kind: 'polyline',
        points: [
          [13, 9],
          [13, 13],
        ],
      },
    ],
  },
  'kw-reach': {
    title: 'Reach',
    elements: [
      {
        kind: 'polyline',
        points: [
          [7, 5],
          [17, 5],
        ],
      },
      {
        kind: 'polyline',
        points: [
          [12, 6],
          [12, 20],
        ],
      },
      {
        kind: 'polyline',
        points: [
          [8, 11],
          [12, 7],
          [16, 11],
        ],
      },
    ],
  },
  'kw-vigilance': {
    title: 'Vigilance',
    elements: [
      {
        kind: 'polygon',
        points: [
          [5, 12],
          [12, 7],
          [19, 12],
          [12, 17],
        ],
      },
      { kind: 'circle', cx: 12, cy: 12, r: 2.4, fill: true },
    ],
  },
  'kw-haste': {
    title: 'Haste',
    elements: [
      {
        kind: 'polygon',
        points: [
          [14, 4],
          [6, 14],
          [11, 14],
          [9, 20],
          [18, 9],
          [12, 9],
        ],
        fill: true,
      },
    ],
  },
  'kw-first_strike': {
    title: 'First strike',
    elements: [
      {
        kind: 'polyline',
        points: [
          [6, 6],
          [6, 18],
        ],
      },
      {
        kind: 'polyline',
        points: [
          [6, 12],
          [19, 12],
        ],
      },
      {
        kind: 'polyline',
        points: [
          [15, 8],
          [19, 12],
          [15, 16],
        ],
      },
    ],
  },
  'kw-trample': {
    title: 'Trample',
    elements: [
      {
        kind: 'polyline',
        points: [
          [6, 6],
          [12, 12],
          [18, 6],
        ],
      },
      {
        kind: 'polyline',
        points: [
          [6, 11],
          [12, 17],
          [18, 11],
        ],
      },
      {
        kind: 'polyline',
        points: [
          [5, 20],
          [19, 20],
        ],
      },
    ],
  },
  'kw-deathtouch': {
    title: 'Deathtouch',
    elements: [
      { kind: 'circle', cx: 12, cy: 12, r: 3, fill: true },
      {
        kind: 'polyline',
        points: [
          [12, 4],
          [12, 8],
        ],
      },
      {
        kind: 'polyline',
        points: [
          [12, 16],
          [12, 20],
        ],
      },
      {
        kind: 'polyline',
        points: [
          [4, 12],
          [8, 12],
        ],
      },
      {
        kind: 'polyline',
        points: [
          [16, 12],
          [20, 12],
        ],
      },
      {
        kind: 'polyline',
        points: [
          [6.5, 6.5],
          [9, 9],
        ],
      },
      {
        kind: 'polyline',
        points: [
          [15, 15],
          [17.5, 17.5],
        ],
      },
      {
        kind: 'polyline',
        points: [
          [17.5, 6.5],
          [15, 9],
        ],
      },
      {
        kind: 'polyline',
        points: [
          [9, 15],
          [6.5, 17.5],
        ],
      },
    ],
  },
  'kw-lifelink': {
    title: 'Lifelink',
    elements: [
      {
        kind: 'polygon',
        points: [
          [12, 19],
          [5, 11],
          [5, 8],
          [8, 6],
          [12, 9],
          [16, 6],
          [19, 8],
          [19, 11],
        ],
        fill: true,
      },
    ],
  },
  'kw-double_strike': {
    title: 'Double strike',
    elements: [
      {
        kind: 'polyline',
        points: [
          [6, 7],
          [11, 12],
          [6, 17],
        ],
      },
      {
        kind: 'polyline',
        points: [
          [13, 7],
          [18, 12],
          [13, 17],
        ],
      },
    ],
  },

  // ── State + seat/ready markers ──────────────────────────────────────────────
  tap: {
    title: 'Tapped',
    strokeWidth: 2.4,
    elements: [
      { kind: 'polyline', points: TAP_SWEEP },
      {
        kind: 'polyline',
        points: [
          [TAP_HEAD_TIP[0] - 4, TAP_HEAD_TIP[1] - 2.5],
          TAP_HEAD_TIP,
          [TAP_HEAD_TIP[0] - 0.5, TAP_HEAD_TIP[1] - 5],
        ],
      },
    ],
  },
  ready: {
    title: 'Ready',
    strokeWidth: 2.6,
    elements: [
      {
        kind: 'polyline',
        points: [
          [5, 13],
          [10, 18],
          [19, 7],
        ],
      },
    ],
  },
  seat: {
    title: 'Seat',
    elements: [
      { kind: 'circle', cx: 12, cy: 8, r: 3.2 },
      {
        kind: 'polyline',
        points: [
          [6, 20],
          [8, 14],
          [16, 14],
          [18, 20],
        ],
      },
    ],
  },
} as const satisfies Record<string, GlyphDef>;

/** The name of any glyph in the vocabulary — the keys of {@link GLYPHS}. */
export type GlyphName = keyof typeof GLYPH_DEFS;

/**
 * The vocabulary, keyed by name. Exposed under the widened {@link GlyphDef} type
 * (the `as const` literal above preserves the key union for {@link GlyphName} but
 * drops optional properties like `strokeWidth` from each entry's type — this view
 * restores them for consumers, without losing the compile-time key checking).
 */
export const GLYPHS: Record<GlyphName, GlyphDef> = GLYPH_DEFS;

/** All glyph names, sorted for stable enumeration (galleries, tests). */
export const GLYPH_NAMES = Object.keys(GLYPHS).sort() as GlyphName[];

/**
 * The glyph name for an engine keyword wire name (e.g. `"first_strike"` →
 * `"kw-first_strike"`), or `null` when the vocabulary has no glyph for it. The
 * catalog-coverage test asserts this never returns `null` for a shipped keyword, so
 * a new catalog keyword without a glyph fails CI rather than rendering an empty gap.
 */
export function keywordGlyphName(keyword: string): GlyphName | null {
  const name = `kw-${keyword}`;
  return name in GLYPHS ? (name as GlyphName) : null;
}
