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

/**
 * Card-face state indicators (issue #320): the keyword-glyph strip color, the
 * latent activated-ability marker dot, and the marked-damage badge. These read
 * against the card body and, per ui-requirements §10, stay distinct from the
 * selection (blue ring), targeting (orange ring), and playable (gold edge bar)
 * accents by **shape** — a glyph strip, a small dot, a corner badge — not hue, so a
 * colorblind player separates them without color vision. The ability marker's hue is
 * deliberately a muted violet, well away from the gold playable bar it must never be
 * confused with (latent vs live).
 */
export const INDICATORS = {
  /** Keyword-glyph stroke color — a legible neutral on the dark card body. */
  keyword: '#C6CBD2',
  /** The latent activated-ability marker dot (muted violet — not the gold bar). */
  abilityMarker: '#A99BC4',
  /** Marked combat damage badge fill. */
  damageBg: '#B0413A',
  /** Marked combat damage badge text. */
  damageText: '#F6E7E4',
  /**
   * Combat-declaration indicators (issue #332). Like the other card-face accents
   * these stay distinct from selection (ring), targeting (ring), and playable (bottom
   * edge bar) by **shape** — an attacker wears a bar on the *top* edge, a blocker a
   * bar on the *left* edge — so a colorblind player separates them without hue. Hues
   * are combat-warm (attacker) and defender-cool (blocker) as a bonus, not the signal.
   */
  attackingBar: '#E4572E',
  blockingBar: '#3F7FC4',
} as const;

/**
 * The blocker→attacker combat link (issue #339): a canvas-layer connector drawn
 * between a blocker and the attacker it blocks. It stays distinct from the selection
 * ring, the targeting ring/arrow, and the playable edge bar by **shape** — it is a
 * *doubled* (two parallel) stroke with a small node at the blocker end, not a single
 * line or a full-perimeter ring — so a colorblind player separates it from those
 * accents without relying on hue. The warm combat hue matches the attacker bar
 * (#332) as a bonus, not the signal. Purely presentational: it renders exactly the
 * scene's server-derived `combatLinks`, computing no combat.
 */
export const COMBAT_LINK = {
  /** Stroke color — the combat-warm hue, shared with the attacker bar. */
  color: '#E4572E',
  /** Width of each of the two parallel strokes (logical px). */
  strokeWidth: 2,
  /** Gap between the two parallel strokes — the "doubled" look that reads as a bind. */
  gap: 3,
  /** Radius of the node drawn at the blocker end, marking the link's direction. */
  nodeRadius: 4,
  /** Alpha for links at full emphasis (few links, or an isolated participant's links). */
  alpha: 0.9,
  /** Alpha for links on a crowded board with nothing isolated — present but calmed so
   * the board stays legible until focus isolates one object's links. */
  crowdedAlpha: 0.32,
  /** Above this many links the board is "crowded": links calm to `crowdedAlpha` unless
   * a participant is focused/selected, which isolates its links at full `alpha`. */
  crowdedThreshold: 6,
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
  /** Tap is a *slight* dim riding the partial rotation (blueprint: one tap
   * treatment everywhere) — legibility of a tapped board state stays high. */
  tappedAlpha: 0.8,
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

/**
 * Card size tiers (blueprint §Card vocabulary): hand (largest) → field (your
 * battlefield at a duel) → support → mini (the stepped-down dense tier the
 * density ladder engages) → chip (land digests). The *set of faces* never
 * changes; which tier a surface uses is the shell layout's call.
 */
export const TIER = {
  chip: { w: 44, h: 60 },
  mini: { w: 54, h: 76, name: 9, mono: 16, pip: 10, header: 24, type: 8 },
  support: { w: 66, h: 92, name: 11, mono: 22, pip: 12, header: 30, type: 9 },
  field: { w: 84, h: 118, name: 11, mono: 30, pip: 13, header: 34, type: 10 },
  hand: { w: 104, h: 146, name: 12, mono: 38, pip: 15, header: 40, type: 11 },
} as const;

/**
 * The tap treatment (blueprint §Card vocabulary): ONE treatment at every tier —
 * a partial rotation plus a slight dim — the same visual for you and opponents,
 * rendered as a tween in the live client. Partial rotation is what keeps small
 * cards legible; the row gap absorbs the swept corners.
 */
export const TAP = {
  /** Tap rotation in radians (~25°). */
  angle: (25 * Math.PI) / 180,
} as const;

export type ColorIdentity = keyof typeof PALETTE;
