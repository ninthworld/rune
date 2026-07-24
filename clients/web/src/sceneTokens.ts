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
 * interaction plus the priority holder, never decorative; selection keeps its
 * own family because it co-occurs with targeting on screen.
 */
export const SCENE_HUES = {
  gold: {
    value: '#F2C94C',
    meaning: 'you can act',
    states: ['actionable', 'priority'],
  },
  blue: {
    value: '#7FB2E5',
    meaning: 'your attention',
    states: ['selection'],
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

/** The palette slots one environment theme fills (art itself is issue #471's;
 * these are the value slots the three parallax groups and the arena read). */
export interface EnvironmentTheme {
  /** Display name. */
  label: string;
  /** Sky gradient, zenith stop. */
  skyTop: string;
  /** Sky gradient, horizon stop. */
  skyHorizon: string;
  /** Sky gradient, base/vignette stop. */
  skyBase: string;
  /** Far ground — silhouetted landforms. */
  ground: string;
  /** Arena edge — the play surface's surround. */
  arena: string;
  /** The theme's ambient glow accent (environmental, never a state channel). */
  glow: string;
}

/**
 * The three launch theme concepts (visual-system §4) as data — no theme art in
 * this layer. Every theme must pass the same check: text and accents hit their
 * contrast budgets against its slots with no per-theme retuning (enforced by
 * the unit test); the environment stays at least one contrast step below the
 * plane's content.
 */
export const SCENE_THEMES = {
  /** Runic Vale (default) — indigo sky, slate arena, cool teal glow. */
  runicVale: {
    label: 'Runic Vale',
    skyTop: '#2C3A55',
    skyHorizon: '#1B2233',
    skyBase: '#12141C',
    ground: '#161C29',
    arena: '#222B39',
    glow: '#4E9A9B',
  },
  /** Ember Reach — deep umber sky, basalt arena, warm ember accents. */
  emberReach: {
    label: 'Ember Reach',
    skyTop: '#4A3226',
    skyHorizon: '#2E211A',
    skyBase: '#1A1412',
    ground: '#241A16',
    arena: '#2B2622',
    glow: '#C97B4A',
  },
  /** Pale Court — blue-gray dawn, weathered marble arena, faint gilt. */
  paleCourt: {
    label: 'Pale Court',
    skyTop: '#55606F',
    skyHorizon: '#3A424E',
    skyBase: '#232830',
    ground: '#2E343D',
    arena: '#4A4C50',
    glow: '#C9B37E',
  },
} as const satisfies Record<string, EnvironmentTheme>;

/** A launch theme's key. */
export type SceneThemeName = keyof typeof SCENE_THEMES;

/** The default theme (visual-system §4). */
export const DEFAULT_SCENE_THEME: SceneThemeName = 'runicVale';

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
