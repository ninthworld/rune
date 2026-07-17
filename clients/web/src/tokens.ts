/**
 * Design tokens shared by BOTH card renderers (the Pixi factory and the HTML
 * card component). Locked in docs/design/ui-design-notes.md — change there first.
 */
export const PALETTE = {
  W: '#CFC7AC',
  U: '#4E86C1',
  B: '#77688C',
  R: '#C05B4D',
  G: '#57935F',
  M: '#C9A84C', // multicolor
  C: '#8C949C', // colorless
  L: '#A08A6E', // land
} as const;

export const PT_TEXT = {
  W: '#2A2820',
  U: '#0D1F33',
  B: '#17111F',
  R: '#2E0D08',
  G: '#122015',
  M: '#2E240A',
  C: '#1C2024',
  L: '#241C12',
} as const;

export const SURFACES = {
  board: '#15171A',
  cardBody: '#23262B',
  nameText: '#E8E6E1',
  typeText: '#9BA0A8',
  selection: '#7FB2E5',
  targeting: '#E0784A',
} as const;

/**
 * The always-on "this card has an offered action" affordance (issue #277) —
 * playable hand cards and permanents with an activatable ability. It must read as
 * distinct from the selection ring (`SURFACES.selection`) and the targeting ring
 * (`SURFACES.targeting`) WITHOUT relying on hue, per ui-requirements §10: it is a
 * solid **bottom-edge bar**, a different *shape* than the full-perimeter rings, so
 * it stays legible to a colorblind player who cannot separate the accent colors.
 * Purely presentational — driven only by `RenderedCard.actions.length > 0`, never
 * by any client-side legality.
 */
export const AFFORDANCE = {
  /** Accent color of the playable edge bar (warm gold — distinct hue as a bonus). */
  actionable: '#F2C94C',
  /** Height of the bottom edge bar in logical px (the weight that reads at a glance). */
  edgeHeight: 5,
} as const;

/**
 * Mana pip swatches: `bg` fills the pip disc, `fg` colors the symbol glyph.
 * `N` is the neutral swatch used for generic/numeric and any unrecognized symbol
 * (e.g. `{2}`, `{C}`, hybrid). Colored single-letter symbols use their own key.
 */
export const PIP = {
  W: { bg: '#F1EBD4', fg: '#4A4636' },
  U: { bg: '#AFCBE9', fg: '#17324E' },
  B: { bg: '#A79DB5', fg: '#2A2233' },
  R: { bg: '#E5A192', fg: '#4A170E' },
  G: { bg: '#A3C095', fg: '#1E3320' },
  N: { bg: '#CACBCF', fg: '#26262A' },
} as const;

/** Small chip drawn at a card corner for counters and state (summoning sick). */
export const BADGE = {
  bg: '#3A3E45',
  text: '#D8DBDF',
  stroke: '#565B63',
  counterBg: PALETTE.M,
  counterText: PT_TEXT.M,
} as const;

/** Vector frame geometry — the look of the card body with no images or WotC art. */
export const FRAME = {
  borderWidth: 1.5,
  radius: 8,
  chipRadius: 6,
  headerRadius: 5,
  headerTintAlpha: 0.16,
  monogramAlpha: 0.22,
  selectionWidth: 2,
  tappedAlpha: 0.55,
  sickAlpha: 0.85,
  /** Alpha for a card dimmed as an ineligible target during targeting mode. */
  dimmedAlpha: 0.32,
} as const;

/**
 * Typography tokens. `charWidthRatio` is the average glyph advance as a fraction
 * of font size; the Pixi factory uses it to estimate text extents for layout so
 * it never needs a live canvas/GPU text measurement (keeps it headless-testable).
 *
 * `bitmapName`/`bitmapBaseSize` configure the shared, cached `BitmapFont` the card
 * factory rasterizes ONCE and draws all card text from (ui-requirements §11: "all
 * text in the Pixi layer via cached bitmap text"). The atlas is generated white at
 * `bitmapBaseSize` (the largest glyph we ever display — the hand-tier monogram) so
 * every label can be tinted to its token color and scaled DOWN without re-rasterizing.
 */
export const FONT = {
  family: 'system-ui, sans-serif',
  weight: '500',
  charWidthRatio: 0.55,
  bitmapName: 'RuneCard',
  bitmapBaseSize: 42,
} as const;

/** Card size tiers: digest (opponent chips) / field / support / hand / full (inspect). */
export const TIER = {
  chip: { w: 44, h: 60 },
  support: { w: 66, h: 92, name: 11, mono: 22, pip: 12, header: 30, type: 9 },
  field: { w: 84, h: 118, name: 11, mono: 30, pip: 13, header: 34, type: 10 },
  hand: { w: 104, h: 146, name: 12, mono: 38, pip: 15, header: 40, type: 11 },
} as const;

export type ColorIdentity = keyof typeof PALETTE;
