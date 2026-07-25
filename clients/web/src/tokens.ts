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
  /** Slate edge of the Rune frame (card-representation §3.1). */
  frameEdge: '#2E343A',
  /** The bottom paper-thickness edge and every shadow-side rim (§3.1, §3.11). */
  frameEdgeShade: '#1B2024',
  /** Parchment of the discrete plates — title, type, rules, P/T, glyphs (§3.1). */
  plate: '#DED8CB',
  /** Ink on a parchment plate (§3.5). */
  plateInk: '#191C20',
  /** The battlefield permanent's status band slate (§3.9). */
  statusBand: '#2F3438',
  /** The full card's mana cost disc (§3.5). */
  costDisc: '#20262B',
  /** The top-edge tab plate: `TOKEN`, `×N`, `TOKEN ×N` (§7.4). */
  tokenTab: '#20262B',
} as const;

/**
 * The frame's warm-gold hairline family (`docs/design/card-representation.md`
 * §3.1/§3.11). One implied key light: the rule is lit on its top/left run and
 * `ruleShade` on the bottom/right run, and every parchment plate carries a
 * `plateRim` hairline. Gold is structural, never a color-identity channel —
 * identity only *tints* the rule (§3.4).
 */
export const RUNE_GOLD = {
  /** Lit gold hairline. */
  rule: '#C7A46A',
  /** The rule's shadow-side run. */
  ruleShade: '#8A7042',
  /** Hairline rim around a parchment plate. */
  plateRim: '#B9955E',
} as const;

/**
 * The card **back** field (card-representation §13). Hidden-information safety
 * is the hard requirement: one back for every hidden card on the device, with
 * nothing about it varying with the card it hides. The back's own composition
 * (emblem, rivets, skin manifest) is issue #548's; these are the colors it
 * paints with.
 */
export const CARD_BACK = {
  /** Navy-slate field. */
  field: '#2B3340',
  /** Centred rune-spiral emblem. */
  emblem: '#C7A46A',
  /** The four corner rivet dots on the inset rule. */
  rivet: '#C7A46A',
} as const;

/**
 * Rune frame geometry (card-representation §3.1, §5), as unitless fractions of
 * the card **width `W`** — one number serves every tier, and the face resolves
 * them to px. Measured off the approved `rune-card-states.jpg` baseline at the
 * `Wref = 190 px` authoring width.
 *
 * The frame is deliberately two silhouettes of ONE family (§2): a battlefield
 * permanent is a **square plaque** (`aspectPermanent`), a card in hand or on the
 * stack is a portrait card (`aspectFull`), and a land is a wide **resource
 * tile** (`aspectLandTile`). The square is the portrait frame with its rules
 * area structurally removed.
 */
export const RUNE_FRAME = {
  /** Full-card w ÷ h — hand, stack, inspect. */
  aspectFull: 0.715,
  /** Battlefield permanent w ÷ h — the square plaque. */
  aspectPermanent: 1.0,
  /** Land resource tile w ÷ h. */
  aspectLandTile: 1.45,
  /** Outer corner radius ÷ W. */
  radius: 0.07,
  /** Plate / art-window corner radius ÷ W. */
  plateRadius: 0.035,
  /** Slate edge on left/right/top ÷ W. */
  edge: 0.037,
  /** The bottom paper-thickness edge ÷ W — thicker and one shade darker. */
  edgeBottom: 0.057,
  /** Gold hairline weight ÷ W. */
  rule: 0.01,
  /** Gold hairline inset from the card edge ÷ W. */
  ruleInset: 0.063,
  /** The token silhouette's arch rise above the card's top line ÷ H. */
  archRise: 0.09,
  /** Procedural art-window monogram size ÷ W (the ADR 0024 empty state). */
  monogram: 0.42,
  /** Selection ring weight ÷ W (card-representation §6.1 panel 3). */
  selectRing: 0.021,
  /** Selection outer bloom spread ÷ W — the bloom reads by spread, not hue. */
  selectGlow: 0.05,
  /** Target-candidate ring weight ÷ W (§6.2) — thinner than selection, so the
   * two are separated by weight as well as hue. */
  targetRing: 0.016,
  /** Attachment cluster scale relative to its host (§6.1 panel 6). */
  attachScale: 0.7,
} as const;

/**
 * Band stack of the **full card** (hand, stack, inspect) as fractions of card
 * height `H`, top to bottom (card-representation §3.2, measured on states panel
 * 9). The remainder — outer edges and the four gold rules — is `RUNE_FRAME`.
 */
export const RUNE_BANDS_FULL = {
  /** Title bar: name left, cost disc right. */
  title: 0.077,
  /** Art window — the dominant band. */
  art: 0.482,
  /** Type bar. */
  type: 0.077,
  /** Rules area, carrying the server `rules_text` verbatim. */
  rules: 0.27,
} as const;

/**
 * Band stack of the **battlefield permanent** as fractions of `H` (= `W`)
 * (card-representation §3.3, measured on states panel 1). The permanent face
 * carries **no mana cost and no type bar** — both are absent in every permanent
 * across all three approved baselines, and type identity is carried by the
 * status band's glyph plates and by inspect.
 */
export const RUNE_BANDS_PERM = {
  /** Title bar: name only. */
  title: 0.1,
  /** Art window — the dominant band. */
  art: 0.647,
  /** Status band: glyph plates left, P/T plate right. */
  status: 0.137,
} as const;

/**
 * Card type scale (card-representation §5, §8.4) as fractions of `W`, plus the
 * two hard px floors from `presentation-budgets.md` §Accessibility. Ratios are
 * authored at `Wref = 190`; below that, type **clamps to the floor and the band
 * that holds it grows**, with the art window absorbing the difference — the
 * floor rule, implemented once in `card/dom/theme.ts` so no surface re-derives
 * it.
 */
export const RUNE_TYPE = {
  /** Card name ÷ W. */
  name: 0.074,
  /** Type line ÷ W. */
  typeLine: 0.056,
  /** Rules text ÷ W. */
  rules: 0.056,
  /** P/T numerals ÷ W. */
  pt: 0.115,
  /** Cost-disc numeral ÷ W. */
  cost: 0.085,
  /** Top-edge tab text (`TOKEN`, `×N`) ÷ W. */
  tab: 0.075,
  /** Badge text ÷ W. */
  badge: 0.07,
  /** Card names never render below this (px). */
  floorName: 11,
  /** Critical values — P/T, counts — never render below this (px). */
  floorValue: 12,
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
  /** Marked combat damage badge fill (card-representation §5, measured red). */
  damageBg: '#8E3A2A',
  /** Marked combat damage badge text. */
  damageText: '#F6E7E4',
  /** Counter badge fill (card-representation §5, measured green). */
  counterBg: '#2A5436',
  /** Counter badge text. */
  counterText: '#EAF3E9',
  /**
   * Selection ring core. Canonical `SURFACES.selection` — the maintainer ruling
   * recorded in card-representation §15.1 assigns **blue** to selection and
   * **orange** to targeting, superseding the approved sheets' violet ring; only
   * the hue moved, the transcribed geometry is unchanged.
   */
  selectRing: '#7FB2E5',
  /** Selection outer bloom — the same hue at bloom alpha; **spread**, not hue,
   * separates the bloom from the ring. */
  selectGlow: '#7FB2E5',
  /** The drawn targeting path — canonical `SURFACES.targeting`. */
  targetPath: '#E0784A',
  /** The reticle on a chosen target — the same hue as the path; **geometry**
   * separates chosen (ring + path + reticle) from candidate (ring + pulse). */
  targetReticle: '#E0784A',
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

/**
 * The card-face **art window** (ADR 0024): the reserved region between the
 * header band and the type line that holds the accent monogram procedurally and
 * an illustration when the player's chosen art source has one. Only the two
 * larger tiers draw art — the dense tiers (chip/mini/support) keep their full
 * procedural information budget (ui-design-notes §Card render).
 */
/** The declared ratio (w ÷ h) of a screen-space illustration window. */
const ART_PANEL_ASPECT = 4 / 3;
/** The declared ratio (w ÷ h) of a whole printed card (63 × 88 mm). */
const ART_CARD_ASPECT = 63 / 88;

export const ART = {
  /**
   * Corner radius of the **screen-space** art slot's mask. The card frame's own
   * art window takes `RUNE_FRAME.plateRadius · W` instead, so it scales with the
   * tier; this is the fixed radius of the inspect surfaces' reserved slot.
   */
  radius: 4,
  /** Alpha of the card-body scrim drawn behind the keyword strip over art. */
  scrimAlpha: 0.72,
  /**
   * Tiers whose frame draws an illustration in its art window when one is
   * available (card-representation §12: the art window is the frame's dominant
   * band at every framed tier). The window is ONE node whether it holds an
   * illustration or the procedural color-identity field, so widening this list
   * costs no DOM budget. `chip` is deliberately absent — a digest chip stays
   * procedural in every art mode (§12).
   */
  tiers: ['mini', 'support', 'field', 'hand', 'stack', 'inspect'],
  /**
   * The focal anchor of the window mask (issue #527), as fractions of the mask
   * box, published as `object-position`. Illustrations are cover-fitted — the
   * raster fills the mask and the excess is cropped, never letterboxed and
   * never stretched — so the crop needs a declared anchor. It sits slightly
   * **above** center because a card illustration's subject almost always does;
   * an anchor is a design decision, not a per-image measurement, so the mask
   * behaves identically for every intrinsic size and ratio.
   */
  focusX: 0.5,
  focusY: 0.42,
  /**
   * The crop anchor of ADR 0024 **full-card** mode, as fractions of the frame
   * box, published as a separate `object-position`. A whole printed card is not
   * an illustration, so it must not take {@link ART.focusY}'s focal anchor:
   * card-representation §12 / §16 (decision 16) require the **top 72%** of the
   * printed card — its name band plus its art — to survive the crop into the
   * battlefield's square footprint, and Rune's status band draws over what is
   * lost at the bottom.
   *
   * The value is derived, not chosen. The printed card is `RUNE_FRAME.aspectFull`
   * (0.715) and the battlefield silhouette is square, so a cover fit at the
   * frame's width shows exactly `0.715 / 1.00` = 71.5% ≈ 72% of the printed
   * height. Anchoring that band at the **top** (`y = 0`) is therefore the one
   * position that yields §12's stated crop; the shipped focal anchor (42%) slid
   * the window down to roughly source `y = 6%–78%` and cut away most of the
   * title band. Horizontally the printed card fills the box exactly, so `x`
   * stays centered and is inert.
   */
  fullFocusX: 0.5,
  fullFocusY: 0,
  /**
   * The DECLARED aspect ratio (w ÷ h) of a screen-space art window — the
   * inspect tier and the inspect panel, where the mask has no frame to derive a
   * height from. It overrides the image's natural ratio, so the panel's box is
   * the same before and after any image loads (issue #527).
   */
  panelAspect: ART_PANEL_ASPECT,
  /**
   * The DECLARED aspect ratio (w ÷ h) of a whole-card image in ADR 0024's
   * full-card mode: the printed card proportion (63 × 88 mm). Same purpose as
   * {@link ART.panelAspect} — the box never comes from the file.
   */
  cardAspect: ART_CARD_ASPECT,
  /**
   * The DECLARED aspect ratio (w ÷ h) of the **permanently reserved art slot**
   * on the screen-space inspect surfaces (issue #527). One slot, one size, for
   * every state: no art, art still downloading, art arrived, and *either* art
   * mode. A per-mode reservation would fix the late-load shift but not the
   * mode-switch shift, so the slot takes the **taller** of the two modes (the
   * smaller w ÷ h) and the shorter one is centered inside it — the reserved
   * rectangle then contains both modes at a given width and never changes.
   */
  slotAspect: Math.min(ART_PANEL_ASPECT, ART_CARD_ASPECT),
  /**
   * Font size (logical px) of the reserved slot's empty-state monogram — the
   * same procedural placeholder mark the card frame draws in its art window, so
   * a text-only card reads as a card with no illustration rather than a hole.
   */
  slotMonogram: 44,
} as const;

/**
 * Frame alphas — the state channels that ride opacity rather than geometry.
 * The frame's *sizes* all moved to {@link RUNE_FRAME} as fractions of `W`
 * (card-representation §3.1/§5), so one authored ratio serves every tier.
 *
 * `sickAlpha` is gone on purpose: card-representation §6.2 replaces the
 * summoning-sickness dim with a dedicated glyph plate in the status band, so
 * the state survives at every tier and never competes with tap.
 */
export const FRAME = {
  /** Alpha of the procedural art-window monogram (ADR 0024 empty state). */
  monogramAlpha: 0.22,
  /** Tap is a *slight* dim riding the partial rotation (blueprint: one tap
   * treatment everywhere) — legibility of a tapped board state stays high. */
  tappedAlpha: 0.8,
  /** Alpha for a card dimmed as an ineligible target during targeting mode. */
  dimmedAlpha: 0.32,
} as const;

/**
 * The ×N pile splay (`docs/design/visual-system.md` §5: a fold renders as "a
 * slightly splayed physical pile (2–3 px offsets) with the count badge" —
 * four Plains look like a stack of Plains, not a card wearing arithmetic).
 *
 * One card edge is drawn per hidden member, offset **down-and-left** by
 * (`stepX` · W, `stepY` · H) with its accent edge `edgePx` further out, so the
 * pile deepens with the fold up to `maxLayers`; the `×N` top tab carries the
 * exact N beyond that. The edges are box-shadow layers, so a pile costs ZERO
 * extra DOM nodes at any count (presentation-budgets §Performance: ≤ 12 nodes
 * per card face).
 *
 * Direction and step are card-representation §5/§15.3: the approved sheets
 * splay two of three depictions down-and-left, and down-and-left keeps the
 * card's right edge clear for the P/T plate and the badge channel. This
 * replaces the shipped 2 px up-and-right offset.
 */
export const SPLAY = {
  /** Horizontal step between successive cards in the pile ÷ W (leftward). */
  stepX: 0.055,
  /** Vertical step between successive cards in the pile ÷ H (downward). */
  stepY: 0.03,
  /** How much further out each card's accent edge sits, px. */
  edgePx: 1,
  /** Card edges drawn behind the top card, capping the pile's silhouette. */
  maxLayers: 3,
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
 * Card size tiers — the canonical 4-player, 1280 × 720 table of
 * `docs/design/card-representation.md` §8.1, replacing the inherited
 * 84 × 118 field / 104 × 146 hand assumptions (issue #529).
 *
 * Two silhouettes, one family (§2): the battlefield tiers (`chip`, `mini`,
 * `support`, `field`) are **square plaques** (`w === h`, `RUNE_FRAME`
 * `aspectPermanent`) and carry a `landH` — the height of the same tier's
 * **land resource tile** at `aspectLandTile`. The screen-space tiers (`hand`,
 * `stack`, `inspect`) are portrait cards at `aspectFull`.
 *
 * `name` / `type` / `rules` / `pt` are the authored font sizes in logical px at
 * this viewport; `card/dom/theme.ts` clamps each to the `RUNE_TYPE` floor and
 * grows the holding band around it (§8.4). A `0` means the tier does not draw
 * that field at all — a battlefield permanent has no type bar and no rules
 * area, which is a normative rule of the frame family, not a truncation.
 *
 * Which tier a surface uses is the shell/plane layout's call, never the face's.
 */
export const TIER = {
  chip: { w: 48, h: 48, landH: 48, name: 0, type: 0, rules: 0, pt: 12 },
  mini: { w: 62, h: 62, landH: 43, name: 11, type: 0, rules: 0, pt: 12 },
  support: { w: 78, h: 78, landH: 54, name: 11, type: 0, rules: 0, pt: 12 },
  field: { w: 96, h: 96, landH: 66, name: 11, type: 0, rules: 0, pt: 13 },
  hand: { w: 116, h: 162, landH: 162, name: 13, type: 11, rules: 11, pt: 14 },
  stack: { w: 104, h: 145, landH: 145, name: 12, type: 11, rules: 11, pt: 13 },
  inspect: { w: 260, h: 364, landH: 364, name: 18, type: 13, rules: 13, pt: 20 },
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
