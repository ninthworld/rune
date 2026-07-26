/**
 * Scene-token gates (issue #480): the contrast floors of
 * `presentation-budgets.md` §Accessibility (text ≥ 4.5:1 against its surface,
 * state indicators ≥ 3:1), the motion-budget caps of §Animation, and the
 * lockstep mirrors — so a palette or duration drift fails CI rather than a
 * review eye. Pairs are tested where the design places each element: text on
 * the foundation surfaces and every theme slot; interaction and seat accents
 * on the plane surfaces they render over.
 */
import { describe, expect, it } from 'vitest';
import { PALETTE } from './tokens';
import {
  SCENE_NEUTRALS,
  SCENE_FRAME_ACCENTS,
  SCENE_HUES,
  SCENE_SEAT_ACCENTS,
  SCENE_ELEVATION,
  SCENE_FOCUS_DIM,
  SCENE_GROUND_PLATE,
  compositeOver,
  withAlpha,
  SCENE_MOTION,
  SCENE_BATCH,
  SCENE_SESSION,
  SCENE_SKIP_THRESHOLD_MS,
  SCENE_THEMES,
  SCENE_THEME_NAMES,
  DEFAULT_SCENE_THEME,
  isSceneThemeName,
  relativeLuminance,
  sceneMotionMs,
  sessionMomentMs,
  contrastRatio,
  type SceneMotionClass,
  type SceneSessionClass,
  type SemanticState,
} from './sceneTokens';

/** The plane surfaces interaction/seat accents render over. */
const PLANE_SURFACES = [SCENE_NEUTRALS.surfaceTop, SCENE_NEUTRALS.surfaceBase];

describe('scene tokens — §2 color system', () => {
  it('mirrors the carried frame accents byte-for-byte (lockstep with the card set)', () => {
    // Visual-system §2: frame accents are "carried verbatim from the shipped
    // tokens". The mirror may never drift from `PALETTE`.
    expect(SCENE_FRAME_ACCENTS).toEqual(PALETTE);
  });

  it('assigns every semantic state to exactly one hue family', () => {
    const states = Object.values(SCENE_HUES).flatMap((f) => f.states as readonly SemanticState[]);
    expect(new Set(states).size).toBe(states.length);
    // The §7 table's card-facing states are all owned by some family.
    for (const state of [
      'actionable',
      'priority',
      'selection',
      'targeting',
      'attacking',
      'blocking',
      'damage',
      'healing',
    ] as const) {
      expect(states).toContain(state);
    }
    // Blue owns selection and priority (maintainer ruling, issue #534): the
    // approved baseline draws the priority ring blue-white and the shipped
    // cluster has always painted it that way. The two never collide, because
    // priority is a ring concentric and OUTSIDE a seat's portrait medallion —
    // a placement nothing else uses — while selection strokes a card's own
    // outline. Blue keeps its own family from targeting, which is the
    // separation that matters: those two co-occur on screen.
    expect(SCENE_HUES.blue.states).toEqual(['selection', 'priority']);
    // Gold is now offered-interaction only; it no longer doubles as priority.
    expect(SCENE_HUES.gold.states).toEqual(['actionable']);
  });

  it('provides six distinct seat accents', () => {
    expect(SCENE_SEAT_ACCENTS).toHaveLength(6);
    expect(new Set(SCENE_SEAT_ACCENTS).size).toBe(6);
    for (const accent of SCENE_SEAT_ACCENTS) {
      expect(accent).toMatch(/^#[0-9A-F]{6}$/i);
    }
  });
});

describe('scene tokens — contrast floors (presentation-budgets §Accessibility)', () => {
  it('computes WCAG contrast (sanity: black vs white is 21:1)', () => {
    expect(contrastRatio('#000000', '#FFFFFF')).toBeCloseTo(21, 0);
    expect(contrastRatio('#FFFFFF', '#000000')).toBeCloseTo(21, 0);
  });

  it('keeps primary text ≥ 4.5:1 on every foundation surface', () => {
    for (const surface of [
      SCENE_NEUTRALS.ink,
      SCENE_NEUTRALS.surfaceTop,
      SCENE_NEUTRALS.surfaceBase,
      SCENE_NEUTRALS.raised,
    ]) {
      expect(contrastRatio(SCENE_NEUTRALS.text, surface)).toBeGreaterThanOrEqual(4.5);
    }
  });

  it('keeps secondary text ≥ 4.5:1 on every foundation surface (it is text, not an indicator)', () => {
    // `textMuted` carries captions and status lines on the pregame surfaces
    // (front-door-and-lobby §5.0), so it is held to the TEXT floor, not the
    // 3:1 indicator floor.
    for (const surface of [
      SCENE_NEUTRALS.ink,
      SCENE_NEUTRALS.surfaceTop,
      SCENE_NEUTRALS.surfaceBase,
      SCENE_NEUTRALS.raised,
    ]) {
      expect(contrastRatio(SCENE_NEUTRALS.textMuted, surface)).toBeGreaterThanOrEqual(4.5);
    }
  });

  it('keeps the pregame surfaces inside their floors (front-door-and-lobby §8.22)', () => {
    // Every pregame element sits on a scene SURFACE — panels and the header bar
    // on `raised`, rows and the identity strip on `surface`, the crest chip's
    // fill over `ink`. The environment is always at least one panel behind, so
    // these are the pairs the pregame surfaces actually introduce. (Text over
    // the environment slots themselves is gated by the theme test above.)
    const pregameSurfaces = [
      SCENE_NEUTRALS.raised,
      SCENE_NEUTRALS.surfaceBase,
      SCENE_NEUTRALS.surfaceTop,
      SCENE_NEUTRALS.ink,
    ];
    for (const surface of pregameSurfaces) {
      expect(contrastRatio(SCENE_NEUTRALS.text, surface)).toBeGreaterThanOrEqual(4.5);
      expect(contrastRatio(SCENE_NEUTRALS.textMuted, surface)).toBeGreaterThanOrEqual(4.5);
    }
    // Seat accents are indicator-class: rings, stripes, and occupancy pips only.
    // They must clear 3:1 on every surface they are drawn over — and they never
    // carry text (§5.10), which is why 3:1 is the right floor.
    for (const accent of SCENE_SEAT_ACCENTS) {
      for (const surface of pregameSurfaces) {
        expect(contrastRatio(accent, surface)).toBeGreaterThanOrEqual(3);
      }
    }
    // The ready bar's gold, selection blue, and the ribbon's outcome hues over a
    // pregame panel or row.
    for (const hue of [SCENE_HUES.gold, SCENE_HUES.blue, SCENE_HUES.red, SCENE_HUES.green]) {
      for (const surface of [SCENE_NEUTRALS.raised, SCENE_NEUTRALS.surfaceBase]) {
        expect(contrastRatio(hue.value, surface)).toBeGreaterThanOrEqual(3);
      }
    }
    // The default theme's surround still backs the stage behind those panels.
    const theme = SCENE_THEMES[DEFAULT_SCENE_THEME];
    expect(contrastRatio(SCENE_NEUTRALS.text, theme.surroundBase)).toBeGreaterThanOrEqual(4.5);
  });

  it('keeps every semantic hue ≥ 3:1 against the plane and raised surfaces', () => {
    for (const family of Object.values(SCENE_HUES)) {
      for (const surface of [...PLANE_SURFACES, SCENE_NEUTRALS.raised, SCENE_NEUTRALS.ink]) {
        expect(contrastRatio(family.value, surface)).toBeGreaterThanOrEqual(3);
      }
    }
  });

  it('keeps every seat accent ≥ 3:1 against the plane surfaces', () => {
    for (const accent of SCENE_SEAT_ACCENTS) {
      for (const surface of PLANE_SURFACES) {
        expect(contrastRatio(accent, surface)).toBeGreaterThanOrEqual(3);
      }
    }
  });
});

/**
 * The ground plate (issue #566).
 *
 * The gates above check text against the FOUNDATION surfaces and against a
 * theme's slots directly. Neither describes the failure the maintainer
 * reported: the pregame draws the wordmark, the arena heading, the status
 * lines, and the empty-state sentence with no plate at all, over an
 * illustrated, light, high-texture floor. The acceptance criterion is a floor
 * "against the *shipped* environment plates, in every environment theme — not
 * against a flat token", so what has to be checked is the **composite** a
 * reader actually sees: the plate laid over each slot.
 *
 * The plate is what makes that checkable at all. There is no single foreground
 * that clears 4.5:1 over both `moonlitRuins`' slate and `runicVale`'s pale sand
 * while staying one palette; compositing a known veil makes the effective
 * surface theme-independent instead.
 */
describe('scene tokens — the ground plate under arena text', () => {
  /** Every slot of every shipped theme, which is every plate text can land on. */
  const slots = SCENE_THEME_NAMES.flatMap((name) =>
    Object.entries(SCENE_THEMES[name])
      .filter(([, value]) => typeof value === 'string' && value.startsWith('#'))
      .map(([slot, value]) => ({ theme: name, slot, value: value as string })),
  );

  /** The surface a reader sees where arena text sits on the plate. */
  const under = (plate: string): string =>
    compositeOver(SCENE_GROUND_PLATE.color, SCENE_GROUND_PLATE.alpha, plate);

  it('composites source-over in sRGB (sanity: 0 and 1 are the two backgrounds)', () => {
    expect(compositeOver('#FFFFFF', 0, '#123456')).toBe('#123456');
    expect(compositeOver('#FFFFFF', 1, '#123456')).toBe('#FFFFFF');
    expect(compositeOver('#000000', 0.5, '#FFFFFF')).toBe('#808080');
  });

  it('holds body and secondary text to 4.5:1 over every slot of every theme', () => {
    expect(slots.length).toBeGreaterThan(40);
    for (const { theme, slot, value } of slots) {
      const surface = under(value);
      expect(
        contrastRatio(SCENE_NEUTRALS.text, surface),
        `text over ${theme}.${slot}`,
      ).toBeGreaterThanOrEqual(4.5);
      // `textMuted` is the one the issue names: "`--pregame-text-muted` in
      // particular has nothing to hold it up". It is body text, so it takes the
      // text floor and not the 3:1 indicator floor.
      expect(
        contrastRatio(SCENE_NEUTRALS.textMuted, surface),
        `muted text over ${theme}.${slot}`,
      ).toBeGreaterThanOrEqual(4.5);
    }
  });

  it('holds the gold lockup to 4.5:1 over every slot of every theme', () => {
    // The `RUNE` wordmark and every arena heading are drawn in gold and are
    // TEXT, not an indicator — "barely distinguishable from the arena" is the
    // first symptom the issue lists.
    for (const { theme, slot, value } of slots) {
      expect(
        contrastRatio(SCENE_HUES.gold.value, under(value)),
        `gold over ${theme}.${slot}`,
      ).toBeGreaterThanOrEqual(4.5);
    }
  });

  it('holds every status hue to the 3:1 indicator floor over every slot', () => {
    for (const hue of Object.values(SCENE_HUES)) {
      for (const { theme, slot, value } of slots) {
        expect(
          contrastRatio(hue.value, under(value)),
          `${hue.value} over ${theme}.${slot}`,
        ).toBeGreaterThanOrEqual(3);
      }
    }
  });

  it('would fail without the plate — the gate is not vacuous', () => {
    // The shipped screen: muted text straight onto Runic Vale's plaza core. If
    // this ever passes, the plate has stopped being what carries the floor and
    // these tests have stopped meaning anything.
    expect(contrastRatio(SCENE_NEUTRALS.textMuted, SCENE_THEMES.runicVale.plazaCore)).toBeLessThan(
      4.5,
    );
    expect(contrastRatio(SCENE_HUES.gold.value, SCENE_THEMES.runicVale.plazaCore)).toBeLessThan(
      4.5,
    );
  });

  it('is the world’s own ink, and stays a veil rather than an opaque panel', () => {
    // A new hue here would be a new surface in a palette that has four.
    expect(SCENE_GROUND_PLATE.color).toBe(SCENE_NEUTRALS.ink);
    // Fully opaque would erase the arena the direction is built on; the plate
    // has to let the environment read through it.
    expect(SCENE_GROUND_PLATE.alpha).toBeLessThan(1);
    expect(withAlpha(SCENE_GROUND_PLATE.color, SCENE_GROUND_PLATE.alpha)).toBe(
      'rgb(13 15 19 / 78.0%)',
    );
  });
});

describe('scene tokens — §3 elevation ladder and focus dim', () => {
  it('seeds the documented ladder: rising lift, rest→lifted→held', () => {
    expect(SCENE_ELEVATION.rest.lift).toBe(0);
    // The §3 seed value: hover/keyboard focus rises ~24 px toward the camera.
    expect(SCENE_ELEVATION.lifted.lift).toBe(24);
    expect(SCENE_ELEVATION.held.lift).toBeGreaterThan(SCENE_ELEVATION.lifted.lift);
    // Only the held level tilts (toward pointer/travel direction).
    expect(SCENE_ELEVATION.rest.tilt).toBe(0);
    expect(SCENE_ELEVATION.lifted.tilt).toBe(0);
    expect(SCENE_ELEVATION.held.tilt).toBeGreaterThan(0);
  });

  it('dims focus with brightness + desaturation and NEVER blur', () => {
    expect(SCENE_FOCUS_DIM.brightness).toBeCloseTo(0.6);
    expect(SCENE_FOCUS_DIM.saturate).toBeLessThan(1);
    expect(SCENE_FOCUS_DIM.saturate).toBeGreaterThan(0);
    // Blur is banned (§3): the treatment has exactly these two knobs.
    expect(Object.keys(SCENE_FOCUS_DIM).sort()).toEqual(['brightness', 'saturate']);
  });
});

describe('scene tokens — §8 motion classes inside the budget caps', () => {
  /** The binding caps from presentation-budgets §Animation, per grammar class. */
  const BUDGET_CAPS: Record<SceneMotionClass, number> = {
    micro: 150,
    tapUntap: 250,
    zoneTravel: 400,
    staging: 500,
    resolution: 600,
    turnFlow: 500,
  };

  it('keeps every class inside its budget window', () => {
    for (const [cls, spec] of Object.entries(SCENE_MOTION)) {
      expect(spec.cap).toBe(BUDGET_CAPS[cls as SceneMotionClass]);
      expect(spec.ms).toBeGreaterThanOrEqual(spec.min);
      expect(spec.ms).toBeLessThanOrEqual(spec.cap);
      expect(spec.ease).toContain('cubic-bezier');
    }
  });

  it('keeps the batch stagger and window inside their caps', () => {
    expect(SCENE_BATCH.staggerCap).toBe(80);
    expect(SCENE_BATCH.windowCap).toBe(800);
    expect(SCENE_BATCH.staggerMs).toBeLessThanOrEqual(SCENE_BATCH.staggerCap);
    expect(SCENE_BATCH.windowMs).toBeLessThanOrEqual(SCENE_BATCH.windowCap);
  });

  it('collapses every class to zero under reduced motion (token-level snap)', () => {
    for (const cls of Object.keys(SCENE_MOTION) as SceneMotionClass[]) {
      expect(sceneMotionMs(cls, true)).toBe(0);
      expect(sceneMotionMs(cls, false)).toBe(SCENE_MOTION[cls].ms);
    }
  });
});

describe('scene tokens — §8 session moments inside the budget caps', () => {
  /** The binding caps from visual-system §8 "Session moments", per row. */
  const SESSION_CAPS: Record<SceneSessionClass, number> = {
    gameStart: 800,
    mulligan: 400,
    handKept: 400,
    reconnect: 300,
    defeat: 600,
    victory: 800,
    returnToLobby: 400,
  };

  it('keeps every session moment inside its documented window', () => {
    for (const [cls, spec] of Object.entries(SCENE_SESSION)) {
      expect(spec.cap).toBe(SESSION_CAPS[cls as SceneSessionClass]);
      expect(spec.ms).toBeLessThanOrEqual(spec.cap);
      expect(spec.ms).toBeGreaterThan(0);
    }
  });

  it('marks the rows that can compose past the skip threshold as skippable', () => {
    expect(SCENE_SKIP_THRESHOLD_MS).toBe(600);
    for (const spec of Object.values(SCENE_SESSION)) {
      if (spec.cap > SCENE_SKIP_THRESHOLD_MS) expect(spec.skippable).toBe(true);
    }
  });

  it('collapses every session moment to zero under reduced motion', () => {
    for (const cls of Object.keys(SCENE_SESSION) as SceneSessionClass[]) {
      expect(sessionMomentMs(cls, true)).toBe(0);
      expect(sessionMomentMs(cls, false)).toBe(SCENE_SESSION[cls].ms);
    }
  });
});

describe('scene tokens — §4/§5 environment themes (environment-system.md)', () => {
  /** Slots text may ever land on. Every other slot is strictly behind the plane. */
  const TEXT_BEARING = ['surroundTop', 'surroundBase'] as const;

  it('ships the four approved theme family members with Runic Vale as the default', () => {
    // environment-system.md §5.3. The approved images name these four; the
    // superseded `emberReach` / `paleCourt` concepts predate them (§12 conflict 1).
    expect(Object.keys(SCENE_THEMES).sort()).toEqual([
      'moonlitRuins',
      'runicVale',
      'sunlitObservatory',
      'verdantCanals',
    ]);
    expect(DEFAULT_SCENE_THEME).toBe('runicVale');
    expect(SCENE_THEME_NAMES).toContain(DEFAULT_SCENE_THEME);
    expect(isSceneThemeName('runicVale')).toBe(true);
    expect(isSceneThemeName('emberReach')).toBe(false);
  });

  it('fills all thirteen §5.4 slots in every theme', () => {
    for (const theme of Object.values(SCENE_THEMES)) {
      const { label, ...slots } = theme;
      expect(label.length).toBeGreaterThan(0);
      expect(Object.keys(slots).sort()).toEqual(
        [
          'glow',
          'medallion',
          'paving',
          'plazaCore',
          'plazaEdge',
          'propCool',
          'propWarm',
          'rim',
          'surroundBase',
          'surroundTop',
          'verge',
          'water',
        ].sort(),
      );
      for (const value of Object.values(slots)) {
        expect(value).toMatch(/^#[0-9A-F]{6}$/i);
      }
    }
  });

  it('keeps primary text ≥ 4.5:1 on every slot it can land on (no per-theme retuning)', () => {
    // §5.4. The environment carries no text of its own (§7), and every chrome
    // surface sits on a foundation neutral — so the only environment slots text
    // can ever land over are the two surround stops that peek past a panel.
    for (const theme of Object.values(SCENE_THEMES)) {
      for (const slot of TEXT_BEARING) {
        expect(contrastRatio(SCENE_NEUTRALS.text, theme[slot])).toBeGreaterThanOrEqual(4.5);
      }
    }
  });

  it('keeps the card body separated from the plaza it sits on (cards stay dominant)', () => {
    // §12 conflict 2: the approved baseline is a LIGHT warm plaza under DARK
    // card frames, so "cards are the highest-contrast objects" is delivered by
    // the card body against the field, not by the frame accent. §5.4's literal
    // wording ("card frame accents ≥ 3:1 against plazaCore") is unachievable
    // against its own §5.3 samples — every accent measures 1.1–2.4:1 on Runic
    // Vale — and is recorded as a conflict rather than met by retuning either
    // set of approved values.
    for (const [name, theme] of Object.entries(SCENE_THEMES)) {
      const separation = contrastRatio(SCENE_NEUTRALS.raised, theme.plazaCore);
      // The floor every sampled theme meets. Moonlit Ruins sits at 2.08:1 — its
      // plaza is nearly as dark as a card body — which is the reportable half of
      // the same conflict.
      expect(separation).toBeGreaterThanOrEqual(2);
      if (name === DEFAULT_SCENE_THEME) {
        // The canonical theme, the only one #530 ships an implementation of,
        // clears the full indicator floor.
        expect(separation).toBeGreaterThanOrEqual(3);
      }
      // A card's own internal contrast always exceeds the environment's busiest
      // internal step, at every theme — the invariant behind "cards remain the
      // highest-contrast objects on screen".
      const cardInternal = contrastRatio(SCENE_NEUTRALS.text, SCENE_NEUTRALS.raised);
      const environmentInternal = Math.max(
        contrastRatio(theme.plazaCore, theme.plazaEdge),
        contrastRatio(theme.plazaCore, theme.paving),
        contrastRatio(theme.plazaCore, theme.medallion),
        contrastRatio(theme.rim, theme.verge),
      );
      expect(cardInternal).toBeGreaterThan(environmentInternal);
    }
  });

  it('keeps every layer inside its §1 local-contrast ceiling, recording the exceptions', () => {
    // The §1 per-layer caps and the §2.3 medallion cap, applied to the sampled
    // palettes. Two Moonlit Ruins values exceed a documented cap; the approved
    // images outrank a derived ceiling (the doc's own precedence table), so they
    // are RECORDED here rather than silently retuned. Fixing the palette without
    // updating this list fails the second assertion, so the record cannot rot.
    const RECORDED_OVER_CAP: readonly [string, string, number][] = [
      ['moonlitRuins', 'medallion', 1.4],
      ['moonlitRuins', 'paving', 1.25],
    ];
    const isRecorded = (theme: string, slot: string): boolean =>
      RECORDED_OVER_CAP.some(([t, s]) => t === theme && s === slot);

    for (const [name, theme] of Object.entries(SCENE_THEMES)) {
      // §1: L1's local contrast inside the focal core is capped at 1.25:1, and
      // §2.3 allows the medallion exactly one step more (1.4:1).
      const checks: [string, number, number][] = [
        ['paving', contrastRatio(theme.paving, theme.plazaCore), 1.25],
        ['plazaEdge', contrastRatio(theme.plazaEdge, theme.plazaCore), 1.25],
        ['medallion', contrastRatio(theme.medallion, theme.plazaCore), 1.4],
        // §1: L2's ceiling.
        ['verge', contrastRatio(theme.rim, theme.verge), 2.0],
      ];
      for (const [slot, ratio, cap] of checks) {
        if (isRecorded(name, slot)) expect(ratio).toBeGreaterThan(cap);
        else expect(ratio).toBeLessThanOrEqual(cap);
      }
    }
  });

  it('keeps the plaza at least one contrast step off its surround', () => {
    // §12 conflict 2's replacement wording for visual-system §1/§4: the
    // environment is "at least one contrast step away from, and lower in chroma
    // than, the play surface" — no longer "darker than".
    for (const theme of Object.values(SCENE_THEMES)) {
      expect(contrastRatio(theme.plazaCore, theme.surroundBase)).toBeGreaterThanOrEqual(1.5);
    }
  });

  it('holds plazaCore luminance inside the band the sampled palettes occupy', () => {
    // §5.4 states [0.28, 0.42] for warm themes and [0.12, 0.30] for cool ones.
    // Two of its own §5.3 samples fall outside those bands — Sunlit Observatory
    // (warm) at 0.209 and Moonlit Ruins (cool) at 0.094 — so the band asserted
    // here is the one the approved values actually occupy, and the divergence is
    // reported rather than papered over. The default theme is additionally held
    // to the documented warm band, which it does meet.
    for (const theme of Object.values(SCENE_THEMES)) {
      const luminance = relativeLuminance(theme.plazaCore);
      expect(luminance).toBeGreaterThanOrEqual(0.09);
      expect(luminance).toBeLessThanOrEqual(0.42);
    }
    const canonical = relativeLuminance(SCENE_THEMES[DEFAULT_SCENE_THEME].plazaCore);
    expect(canonical).toBeGreaterThanOrEqual(0.28);
    expect(canonical).toBeLessThanOrEqual(0.42);
  });
});
