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
  SCENE_MOTION,
  SCENE_BATCH,
  SCENE_THEMES,
  DEFAULT_SCENE_THEME,
  sceneMotionMs,
  contrastRatio,
  type SceneMotionClass,
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
    // Selection keeps its own family (it co-occurs with targeting on screen).
    expect(SCENE_HUES.blue.states).toEqual(['selection']);
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
    // The default theme's slots still back the stage behind those panels.
    const theme = SCENE_THEMES[DEFAULT_SCENE_THEME];
    expect(contrastRatio(SCENE_NEUTRALS.text, theme.skyBase)).toBeGreaterThanOrEqual(4.5);
  });

  it('keeps primary text ≥ 4.5:1 on every slot of every theme (no per-theme retuning)', () => {
    for (const theme of Object.values(SCENE_THEMES)) {
      for (const slot of [
        theme.skyTop,
        theme.skyHorizon,
        theme.skyBase,
        theme.ground,
        theme.arena,
      ]) {
        expect(contrastRatio(SCENE_NEUTRALS.text, slot)).toBeGreaterThanOrEqual(4.5);
      }
    }
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

describe('scene tokens — §4 environment themes', () => {
  it('ships the three launch theme concepts with a default', () => {
    expect(Object.keys(SCENE_THEMES).sort()).toEqual(['emberReach', 'paleCourt', 'runicVale']);
    expect(DEFAULT_SCENE_THEME).toBe('runicVale');
    for (const theme of Object.values(SCENE_THEMES)) {
      for (const slot of [
        theme.skyTop,
        theme.skyHorizon,
        theme.skyBase,
        theme.ground,
        theme.arena,
        theme.glow,
      ]) {
        expect(slot).toMatch(/^#[0-9A-F]{6}$/i);
      }
    }
  });
});
