/**
 * Scene tokens for the 2.5D visual system (issue #480, under ADR 0029/0030) —
 * the token layer for the staged battlefield scene: `docs/design/visual-system.md`
 * §2 (color system), §3 (light/shadow/elevation), §4 (environment themes), and
 * §8 (motion grammar classes, capped by `docs/design/presentation-budgets.md`).
 *
 * This is the third token set of the ADR 0019 discipline, with its own
 * clearly-owned home beside the two shipped sets — neither replaces them:
 *
 * - `src/tokens.ts` — CARD tokens (frames, tiers, face indicators), shared by
 *   both card renderers. The scene set MIRRORS the frame accents (§2 carries
 *   them verbatim); a lockstep test asserts the mirror never drifts.
 * - `src/chrome/tokens.css` — CHROME tokens (screen-space panels, rails,
 *   overlays), CSS custom properties.
 * - `src/sceneTokens.ts` (this file) — SCENE tokens: the plane's world colors,
 *   semantic hue families, seat accents, elevation ladder, environment theme
 *   slots, and motion classes. A TS module like the card set, because the scene
 *   layer builds its styles as data (CSS custom properties assembled at render,
 *   the ADR 0019 pattern the DOM card face established).
 *
 * Lockstep rule (carried): these values change only together with the design
 * document; the contrast floors of `presentation-budgets.md` §Accessibility are
 * enforced by unit test (`sceneTokens.test.ts`), so a palette drift fails CI
 * rather than a review eye. No game logic, no I/O — constants and pure helpers.
 */

// ── §2 Foundation neutrals — the dark table world ────────────────────────────

/** Foundation neutrals (visual-system §2, carried and layered). */
export const SCENE_NEUTRALS = {
  /** Ink — the deepest chrome and badges. */
  ink: '#0D0F13',
  /** The play surface (the plane's felt), radial from center to edge. */
  surfaceTop: '#1B212D',
  /** The play surface's outer/base stop. */
  surfaceBase: '#151A24',
  /** Raised surface / card body (mirrors the card set's body — same world). */
  raised: '#23262B',
  /** Line work, faint end — region bounds, dividers. */
  lineFaint: 'rgba(232, 230, 225, 0.06)',
  /** Line work, strong end. */
  lineStrong: 'rgba(232, 230, 225, 0.14)',
  /** Primary text. */
  text: '#E8E6E1',
  /**
   * Secondary/supporting text — captions, metadata, quiet status lines. Added
   * for the pregame surfaces under `docs/design/front-door-and-lobby.md` §5.0
   * ("if a value is needed and absent, it is added to `sceneTokens.ts` under its
   * lockstep test, not invented in CSS"), mirroring the shipped chrome token
   * `--rune-text-muted` so screen space reads one grey. It stays **body text**,
   * not an indicator: the contrast gate holds it to ≥ 4.5:1 on every foundation
   * surface, exactly like {@link text}.
   */
  textMuted: '#9BA0A8',
} as const;

// ── §2 Frame accents — carried verbatim from the card set ────────────────────

/**
 * The WUBRG frame accents, multicolor, colorless, and land (visual-system §2:
 * "carried verbatim from the shipped tokens"). A card's frame color is game
 * information and belongs to the card set (`src/tokens.ts` `PALETTE`); this
 * mirror exists so scene-layer code reads one token home, and the lockstep test
 * pins it byte-for-byte to the card set — the two can never drift apart.
 */
export const SCENE_FRAME_ACCENTS = {
  W: '#CFC7AC',
  U: '#4E86C1',
  B: '#77688C',
  R: '#C05B4D',
  G: '#57935F',
  M: '#C9A84C',
  C: '#8C949C',
  L: '#A08A6E',
} as const;

// ── §2 Semantic hue families — one meaning-group per hue ─────────────────────

/** The scene states a semantic hue family owns (visual-system §2/§7). Distinct
 * states WITHIN a family separate by shape channels, never by hue alone. */
export type SemanticState =
  | 'actionable'
  | 'priority'
  | 'selection'
  | 'targeting'
  | 'attacking'
  | 'blocking'
  | 'damage'
  | 'destruction'
  | 'healing'
  | 'growth';

/** One semantic hue family: its value, meaning, and the states it owns. */
export interface HueFamily {
  /** The family's color value. */
  value: string;
  /** The meaning-group the family owns. */
  meaning: string;
  /** Every state assigned to this family (shape channels separate them). */
  states: readonly SemanticState[];
}

/**
 * The interaction accents as semantic hue families (visual-system §2): each hue
 * owns one meaning-group. Gold stays disciplined — every currently offered
 * interaction, never decorative; selection keeps its own family because it
 * co-occurs with targeting on screen.
 *
 * **Priority is blue, not gold** (maintainer ruling, issue #534). It was listed
 * under gold here and in `visual-system.md`, but `seat-identity.md` §6.1
 * transcribes approved baseline panel 2 as a blue-white double glow ring, and
 * that is what `live-plane-cluster.module.css` has always drawn — so the token
 * disagreed with both the baseline and the shipped pixels. Gold also already
 * carries the decision-deadline arc, which rides directly on top of the
 * priority band, so keeping both there put two states in one hue in adjacent
 * placements.
 *
 * Priority and selection therefore share the blue family and are separated by
 * shape and placement, never by hue: priority is a ring drawn *concentric and
 * outside* a seat's portrait medallion — a placement no other state uses —
 * while selection is a stroke on a card's own outline plus elevation.
 */
export const SCENE_HUES = {
  gold: {
    value: '#F2C94C',
    meaning: 'you can act',
    states: ['actionable'],
  },
  blue: {
    value: '#7FB2E5',
    meaning: 'your attention',
    states: ['selection', 'priority'],
  },
  orange: {
    value: '#E0784A',
    meaning: 'threat / intent',
    states: ['targeting', 'attacking', 'blocking'],
  },
  red: {
    value: '#D9574A',
    meaning: 'loss moment',
    states: ['damage', 'destruction'],
  },
  green: {
    value: '#6FAF78',
    meaning: 'gain moment',
    states: ['healing', 'growth'],
  },
} as const satisfies Record<string, HueFamily>;

// ── §2 Seat identity accents ─────────────────────────────────────────────────

/**
 * The six muted jewel-tone seat accents (visual-system §2), assigned
 * deterministically by seat order: azure, ember, moss, amethyst, amber, teal.
 * Worn by region bounds, nameplates, crest rings, and combat/target references
 * to a player — **never by cards** (frame color is game information). This is
 * the §2 redesign of the shipped `IDENTITY_ACCENTS` cycle; the shipped module
 * keeps rendering the old client until Phase 1 wiring swaps its consumers.
 */
export const SCENE_SEAT_ACCENTS = [
  '#4D7EC9', // azure
  '#B0563F', // ember
  '#4F8F5C', // moss
  '#8B6FB0', // amethyst
  '#C08B3E', // amber
  '#4E9A9B', // teal
] as const;

// ── §4/§5 The directional relationship grammar ───────────────────────────────

/**
 * The geometry of the relationship grammar
 * (`docs/design/stack-and-relationships.md` §4 and §5) as data.
 *
 * These are **shape-channel** numbers, not hues: every relationship kind is
 * separated by geometry (§4.3, §9.4) and hue is only the fourth channel, so the
 * values that make a relationship readable live here rather than inline in the
 * effects layer. Hues stay in {@link SCENE_HUES}; durations stay in the motion
 * classes and `EFFECT_TIMING`.
 */
export const SCENE_RELATIONSHIP = {
  /**
   * §4.2 device D2 — the **monotonic stroke taper**, the primary direction
   * device: the stroke widens from `taperFrom` at the source to `taperTo` at the
   * destination, linearly along the sampled polyline. It is the only device that
   * is both static (survives reduced motion) and *locally* readable (survives
   * occlusion and bundling), which is why it outranks the arrowhead and the
   * dash-crawl.
   */
  taperFrom: 1.2,
  /** §4.2 D2 — stroke width at the destination end. Must exceed `taperFrom`. */
  taperTo: 3.4,
  /** §5.1 — the filled source cap disc. */
  sourceRadius: 5,
  /** §5.2 — the open target reticle on a card destination. */
  reticleRadius: 14,
  /** §5.2 — the reticle's stroke. */
  reticleWidth: 2,
  /** §5.5 — the inset reticle drawn inside a stack slot's bounds. */
  reticleInsetRadius: 10,
  /** §5.2 — the inward chevron's arm length (the arrowhead inside the ring). */
  chevron: 12,
  /** §5.3 — the crest cap's 90° arc sweep, in radians. */
  crestSweep: Math.PI / 2,
  /** §5.3 — chord count the arc is drawn with (no arc primitive exists). */
  crestChords: 5,
  /** §5.3 — the crest arc's stroke. */
  crestWidth: 3,
  /** §5.4 — the zone bracket's two arms. */
  bracketArm: 12,
  /** §5.4 — the zone bracket's spine. */
  bracketSpine: 28,
  /** §4.3 R5 — the fan node sits this fraction along the trunk. */
  fanAt: 0.4,
  /** §4.3 R5 — the hollow fan node's radius. */
  fanRadius: 6,
  /**
   * §4.5 — the ordering channel on the destination cap. The numeral is the
   * destination's 1-based place in the **server's** target list, drawn as that
   * many pips across the cap's arrival normal. Pips rather than the spec's
   * ①②③ glyphs because the draw program is one pooled `Graphics` and one draw
   * call (§8.1); the glyph form lives on the entry's summary chips and in the
   * accessible name, which are DOM.
   */
  numeralPip: 2.5,
  /** §4.5 — centre-to-centre pitch between numeral pips, logical px. */
  numeralPitch: 7,
  /** §4.3 R9 — the attachment/tether square terminal's side. */
  terminal: 6,
  /** §4.3 R9 — the elbow bracket's stroke. */
  elbowWidth: 1.5,
  /**
   * §4.3 R9 — attachment brackets and source tethers are drawn in **line-work
   * neutral** (`rgba(232, 230, 225, .14)`), never in a relationship hue: the
   * hard separation (D6) from a target path. The color is
   * {@link SCENE_NEUTRALS.text}; this is its alpha.
   */
  lineworkAlpha: 0.14,
  /** §10.3 — the edge indicator chevron drawn for an occluded endpoint. */
  edgeIndicator: 20,
  /** §4.4 — the alpha each path state renders at. */
  alpha: {
    pending: 0.9,
    provisional: 0.9,
    confirmed: 0.9,
    /** The crowded-board calm (mirrors the carried `COMBAT_LINK.crowdedAlpha`). */
    calmed: 0.32,
    /** The scalability floor: caps only, no stroke. */
    endpointOnly: 0.6,
    resolving: 0.9,
  },
} as const;

// ── §3 Elevation ladder and focus dim ────────────────────────────────────────

/** One elevation level: the lift toward the camera and the shadow it casts
 * (transform + shadow move together; one implied key light, high and slightly
 * toward the viewer, so shadows fall gently down-screen). */
export interface ElevationLevel {
  /** Rise toward the camera, logical px. */
  lift: number;
  /** Tilt magnitude in degrees (held only — toward pointer/travel direction). */
  tilt: number;
  /** The level's box-shadow. */
  shadow: string;
}

/** The elevation ladder (visual-system §3 — the Phase 1 token seed). */
export const SCENE_ELEVATION = {
  /** 0 — resting: permanents on the plane; contact shadow, tight and dark. */
  rest: { lift: 0, tilt: 0, shadow: '0 2px 4px rgba(0, 0, 0, 0.45)' },
  /** 1 — lifted: hover / keyboard focus; shadow softens and spreads. */
  lifted: { lift: 24, tilt: 0, shadow: '0 10px 18px rgba(0, 0, 0, 0.4)' },
  /** 2 — held: selected / dragged / being cast; highest lift, widest shadow. */
  held: { lift: 34, tilt: 3, shadow: '0 16px 28px rgba(0, 0, 0, 0.38)' },
  /** Screen space: hand fan, stack, inspect, overlays — drop shadow only. */
  screen: { lift: 0, tilt: 0, shadow: '0 12px 32px rgba(0, 0, 0, 0.5)' },
} as const satisfies Record<string, ElevationLevel>;

/**
 * The focus-dim treatment (visual-system §3): focusing a player or object drops
 * non-relevant regions to ~60% brightness with slight desaturation. **Blur is
 * banned** (cost, legibility, motion sickness) — there is deliberately no blur
 * token, and none may be added.
 */
export const SCENE_FOCUS_DIM = {
  brightness: 0.6,
  saturate: 0.85,
} as const;

// ── §8 Motion grammar classes, inside the budget caps ────────────────────────

/** Easing curves for the motion grammar (settle-into-place is the house feel). */
export const SCENE_EASE = {
  /** Default state-change easing. */
  standard: 'cubic-bezier(0.25, 0.1, 0.25, 1)',
  /** Travel/staging: fast out, soft landing (the contact settle). */
  decelerate: 'cubic-bezier(0.05, 0.7, 0.1, 1)',
  /** Departures (to stack, to graveyard): gentle start, quick exit. */
  accelerate: 'cubic-bezier(0.3, 0, 0.8, 0.15)',
} as const;

/** One motion class: the default duration, its budget window, and easing. */
export interface MotionClassSpec {
  /** Standard-quality default duration, ms. */
  ms: number;
  /** The budget class's floor, ms (`presentation-budgets.md` §Animation). */
  min: number;
  /** The budget class's cap, ms — binding; the test pins `min ≤ ms ≤ cap`. */
  cap: number;
  /** The class's easing curve. */
  ease: string;
}

/** The §8 grammar classes mapped to their budget windows. */
export const SCENE_MOTION = {
  /** Micro feedback: hover lift, selection, legality pulse. */
  micro: { ms: 120, min: 80, cap: 150, ease: SCENE_EASE.standard },
  /** Tap / untap rotation tween. */
  tapUntap: { ms: 200, min: 150, cap: 250, ease: SCENE_EASE.standard },
  /** Zone travel: draw, play, discard, exile, die. */
  zoneTravel: { ms: 320, min: 250, cap: 400, ease: SCENE_EASE.decelerate },
  /** Staging / focus / camera change (scene-geometry tween). */
  staging: { ms: 400, min: 300, cap: 500, ease: SCENE_EASE.decelerate },
  /** Resolution / impact effects (state already applied). */
  resolution: { ms: 500, min: 0, cap: 600, ease: SCENE_EASE.standard },
  /** Turn / phase / priority transitions (non-blocking cues). */
  turnFlow: { ms: 400, min: 0, cap: 500, ease: SCENE_EASE.standard },
} as const satisfies Record<string, MotionClassSpec>;

/** A motion grammar class name. */
export type SceneMotionClass = keyof typeof SCENE_MOTION;

/**
 * The skip threshold of the motion grammar (visual-system §8): a composition
 * completing in ≤ 600 ms is shorter than a deliberate skip and is **not**
 * individually user-skippable; anything that may run past it must be.
 */
export const SCENE_SKIP_THRESHOLD_MS = 600;

/** One session moment: its staged duration, the §8 cap, and skippability. */
export interface SessionMomentSpec {
  /** Standard-motion staged duration, ms. */
  ms: number;
  /** The class's budget cap, ms — binding; the test pins `ms ≤ cap`. */
  cap: number;
  /**
   * Whether the composition may run past {@link SCENE_SKIP_THRESHOLD_MS} and so
   * must be user-skippable (input or setting). Mirrors the *skippable* marks of
   * the §8 "Session moments" table; a row at or under the threshold is never
   * marked, and is still interruptible by a newer authoritative view.
   */
  skippable: boolean;
}

/**
 * The §8 "Session moments" rows as data (issue #509) — the moments that open
 * and close a game, capped exactly as the grammar states them. These are
 * *presentation* windows on events the server already decided; nothing here
 * gates input, and every one collapses to zero under reduced motion through
 * {@link sessionMomentMs}.
 */
export const SCENE_SESSION = {
  /** Game start: environment fades up, regions assemble, hands deal. */
  gameStart: { ms: 800, cap: 800, skippable: true },
  /** Mulligan: hand sweeps back to library, redraw deals. */
  mulligan: { ms: 320, cap: 400, skippable: true },
  /** Keeping a hand: the kept hand settles, bottomed cards travel to library. */
  handKept: { ms: 320, cap: 400, skippable: false },
  /** Reconnect / fast-forward: the single "you are here" pulse after a rebuild. */
  reconnect: { ms: 300, cap: 300, skippable: false },
  /** Concede / defeat: the quiet dim into the verdict panel (loss family). */
  defeat: { ms: 600, cap: 600, skippable: false },
  /** Victory: the gold rune bloom behind the verdict panel. */
  victory: { ms: 800, cap: 800, skippable: true },
  /** Return to lobby: the scene recedes (scale down + dim) into the lobby. */
  returnToLobby: { ms: 400, cap: 400, skippable: false },
} as const satisfies Record<string, SessionMomentSpec>;

/** A session-moment class name. */
export type SceneSessionClass = keyof typeof SCENE_SESSION;

/**
 * A session moment's duration with the reduced-motion collapse wired at the
 * token level, exactly as {@link sceneMotionMs} does for the object classes:
 * reduced motion snaps straight to the end state with no staging at all.
 */
export function sessionMomentMs(cls: SceneSessionClass, reducedMotion: boolean): number {
  return reducedMotion ? 0 : SCENE_SESSION[cls].ms;
}

/** Simultaneous-batch staging (mass untap, board wipe, token swarm). */
export const SCENE_BATCH = {
  /** Per-item stagger, ms (budget cap 80). */
  staggerMs: 60,
  /** The budget's per-item stagger cap. */
  staggerCap: 80,
  /** Total batch window, ms — items beyond it land together. */
  windowMs: 800,
  /** The budget's total-window cap. */
  windowCap: 800,
} as const;

/**
 * A class's duration with the reduced-motion collapse wired at the token level
 * (the carried contract): under reduced motion every animation snaps to its end
 * state — zero duration, zero layout or state difference. Consumers pass their
 * `prefers-reduced-motion` result; nothing downstream needs its own collapse.
 */
export function sceneMotionMs(cls: SceneMotionClass, reducedMotion: boolean): number {
  return reducedMotion ? 0 : SCENE_MOTION[cls].ms;
}

// ── §4 Environment theme palette slots ───────────────────────────────────────

/**
 * The palette slots one environment theme fills
 * (`docs/design/environment-system.md` §5.4). Thirteen slots, replacing the six
 * sky/ground slots that predated the approved baselines: those could not express
 * a **light plaza over a dark surround**, which is what the images actually show
 * (§12 conflict 3). The art itself is issue #548's; these are the value slots the
 * L0–L3 layer contract, the T0 token composition, and the Lite L0 gradient read.
 *
 * Slot → layer, so a reader can place every value:
 *
 * - `surroundTop` / `surroundBase` / `water` → **L0**, the far surround.
 * - `plazaCore` / `plazaEdge` / `paving` / `medallion` → **L1**, the arena floor.
 * - `rim` / `verge` → **L2**, the arena edge.
 * - `propWarm` / `propCool` → **L3**, the corner-anchored props.
 * - `glow` → the theme's ambient accent (environmental, never a state channel).
 */
export interface EnvironmentTheme {
  /** Display name. */
  label: string;
  /**
   * L0's inner gradient stop — the plate-free horizon haze above the surround,
   * and the Lite/T0 radial's centre stop. The one slot §5.3 does not sample (the
   * images are silent above the plate); chosen one step light of
   * {@link surroundBase} and held to the text floor by the contrast gate.
   */
  surroundTop: string;
  /** L0's outer stop — distant foliage and the horizon vignette. */
  surroundBase: string;
  /** L0's water bodies (streams, canals, reflecting pools). */
  water: string;
  /** L1's plaza field at its centre — the bright surface cards sit on. */
  plazaCore: string;
  /** L1's plaza field at its outer edge. */
  plazaEdge: string;
  /** L1's radial paving rings and fan strokes. */
  paving: string;
  /** L1's central rune medallion at `(50 %, 40 %)`, `r = 5 % W`. */
  medallion: string;
  /** L2's stone rim and the two raised lips. */
  rim: string;
  /** L2's grass/ground verge outside the rim. */
  verge: string;
  /** L3's warm props — lanterns, brass, flowering shrubs. */
  propWarm: string;
  /** L3's cool props — crystal plinths, rune veins, glass. */
  propCool: string;
  /** The theme's ambient glow accent (environmental, never a state channel). */
  glow: string;
}

/**
 * The four theme family members of `environment-system.md` §5.3 as data — no
 * theme art in this layer. Every value is **sampled from the approved images**
 * (`docs/ui-concepts/rune-2.5d-interface-baseline.jpg` for Runic Vale, panels
 * 6–8 of `rune-battlefield-environments.jpg` for the rest) and rounded; the
 * images are the rank-1/2 binding sources, so these are transcriptions rather
 * than choices.
 *
 * Every theme passes the same gates with no per-theme retuning (enforced by
 * `sceneTokens.test.ts`): the card body stays separated from the plaza it sits
 * on, primary text clears 4.5:1 on the two slots it can ever land on, and each
 * layer's local contrast stays inside its §1 ceiling. The two sampled values
 * that exceed a documented ceiling are recorded by that test rather than
 * silently retuned — the images outrank a derived cap.
 */
export const SCENE_THEMES = {
  /** Runic Vale (canonical, default) — warm sand plaza, cool teal water, warm
   * lantern gold. The baseline itself. */
  runicVale: {
    label: 'Runic Vale',
    surroundTop: '#5F6746',
    surroundBase: '#565D3C',
    water: '#5F7674',
    plazaCore: '#B4A379',
    plazaEdge: '#A89B72',
    paving: '#B2AB7A',
    medallion: '#9FA991',
    rim: '#54534C',
    verge: '#585C4B',
    propWarm: '#9D7C58',
    propCool: '#36ABBC',
    glow: '#4E9A9B',
  },
  /** Verdant Canals — the same plaza, deeper foliage, bright cyan canals. */
  verdantCanals: {
    label: 'Verdant Canals',
    surroundTop: '#414830',
    surroundBase: '#323723',
    water: '#414432',
    plazaCore: '#8A7F66',
    plazaEdge: '#A08A64',
    paving: '#96895F',
    medallion: '#8F9478',
    rim: '#6F6858',
    verge: '#535A49',
    propWarm: '#837451',
    propCool: '#3FC2E0',
    glow: '#3FC2E0',
  },
  /** Sunlit Observatory — warm ochre and terracotta, pale gold light, brass. */
  sunlitObservatory: {
    label: 'Sunlit Observatory',
    surroundTop: '#665849',
    surroundBase: '#514638',
    water: '#6F5A46',
    plazaCore: '#907B5F',
    plazaEdge: '#A68861',
    paving: '#9B815F',
    medallion: '#B18E54',
    rim: '#836C57',
    verge: '#8A7356',
    propWarm: '#B18E54',
    propCool: '#8FA6B0',
    glow: '#C9A45E',
  },
  /** Moonlit Ruins — cool blue-violet night, grey slate, cyan rune glow. */
  moonlitRuins: {
    label: 'Moonlit Ruins',
    surroundTop: '#3A404A',
    surroundBase: '#2C3238',
    water: '#243A59',
    plazaCore: '#52575E',
    plazaEdge: '#5B6069',
    paving: '#61666E',
    medallion: '#6E7B8C',
    rim: '#384457',
    verge: '#2F4251',
    propWarm: '#7A6A4E',
    propCool: '#3A6A9C',
    glow: '#5379A8',
  },
} as const satisfies Record<string, EnvironmentTheme>;

/** A launch theme's key. */
export type SceneThemeName = keyof typeof SCENE_THEMES;

/** Every theme key, in display order (the settings surface's option order). */
export const SCENE_THEME_NAMES = Object.keys(SCENE_THEMES) as SceneThemeName[];

/** The default theme (`environment-system.md` §5.3 — the canonical baseline). */
export const DEFAULT_SCENE_THEME: SceneThemeName = 'runicVale';

/** Whether a value names a shipped theme (guards a stale stored preference). */
export function isSceneThemeName(value: unknown): value is SceneThemeName {
  return typeof value === 'string' && value in SCENE_THEMES;
}

// ── Contrast helpers — the check every palette value passes ──────────────────

/**
 * WCAG relative luminance of a `#RRGGBB` color. Pure math, exported so the
 * contrast gate that pins these tokens (and, later, any theme added by #471)
 * is computed one way everywhere.
 */
export function relativeLuminance(hex: string): number {
  const channel = (offset: number): number => {
    const value = parseInt(hex.slice(offset, offset + 2), 16) / 255;
    return value <= 0.04045 ? value / 12.92 : Math.pow((value + 0.055) / 1.055, 2.4);
  };
  return 0.2126 * channel(1) + 0.7152 * channel(3) + 0.0722 * channel(5);
}

/** WCAG contrast ratio between two `#RRGGBB` colors (order-independent). */
export function contrastRatio(a: string, b: string): number {
  const la = relativeLuminance(a);
  const lb = relativeLuminance(b);
  const [hi, lo] = la >= lb ? [la, lb] : [lb, la];
  return (hi + 0.05) / (lo + 0.05);
}
