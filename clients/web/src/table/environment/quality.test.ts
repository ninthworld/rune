/**
 * The §7.1, §8, and §1.1 gates of `docs/design/environment-system.md` — the
 * quality matrix, the loading model, and the failure fallbacks (issue #530).
 *
 * The invariant these all serve: **there is no state in which the environment
 * is a hole, a flat black field, or a blocker** (§8.3). `planEnvironment` is
 * total and pure, so that claim is checkable rather than aspirational — every
 * assertion below is an input the resolver must still answer coherently for.
 */
import { describe, expect, it } from 'vitest';
import type { EffectQuality } from '../effects';
import { DEFAULT_SCENE_THEME, SCENE_THEMES, type SceneThemeName } from '../../sceneTokens';
import {
  ENV_EXCURSION_PX,
  ENV_HOOKS,
  ENV_LAYERS,
  ENV_MANIFESTS,
  ENV_TABLET_MAX_WIDTH,
  ENV_VARIANTS,
  ambientLevel,
  baseExcursionPx,
  hooksFor,
  isPortraitViewport,
  planEnvironment,
  type EnvLayerId,
  type EnvManifestKey,
  type EnvironmentPlan,
} from './index';

const DESKTOP = { width: 1680, height: 945 };
const TABLET = { width: 1180, height: 820 };
const PHONE = { width: 390, height: 844 };
const QUALITIES: EffectQuality[] = ['high', 'standard', 'lite'];

function plan(overrides: Partial<Parameters<typeof planEnvironment>[0]> = {}): EnvironmentPlan {
  return planEnvironment({
    theme: DEFAULT_SCENE_THEME,
    quality: 'standard',
    reducedMotion: false,
    viewport: DESKTOP,
    ...overrides,
  });
}

/** A layer's plan by id. */
function layer(p: EnvironmentPlan, id: EnvLayerId) {
  const found = p.layers.find((entry) => entry.layer === id);
  expect(found).toBeDefined();
  return found!;
}

describe('environment quality — §8.1 the quality matrix', () => {
  it('draws all four layers at High and Standard', () => {
    for (const quality of ['high', 'standard'] as const) {
      const p = plan({ quality });
      for (const id of ['l0', 'l1', 'l2', 'l3'] as EnvLayerId[]) {
        expect(layer(p, id).treatment).toBe('plate');
      }
      expect(layer(p, 'l1').variant).toBe('l1');
    }
  });

  it('degrades Lite to the token gradient + half-res L1, with L2 and L3 off', () => {
    const p = plan({ quality: 'lite' });
    expect(layer(p, 'l0').treatment).toBe('token-gradient');
    expect(layer(p, 'l2').treatment).toBe('off');
    expect(layer(p, 'l3').treatment).toBe('off');
  });

  it('NEVER drops L1 — Lite keeps the theme’s identity floor', () => {
    // §8.1: "Lite retains the illustrated identity (the L1 plate) rather than
    // collapsing to a gradient — the requirement from #542 and the reason L1 is
    // never dropped." This is the single most load-bearing row of the matrix.
    for (const quality of QUALITIES) {
      for (const reducedMotion of [false, true]) {
        for (const viewport of [DESKTOP, TABLET, PHONE]) {
          const l1 = layer(plan({ quality, reducedMotion, viewport }), 'l1');
          expect(l1.treatment).not.toBe('off');
        }
      }
    }
    expect(layer(plan({ quality: 'lite' }), 'l1').variant).toBe('l1-half');
  });

  it('always returns four layers in fixed z-order, whatever the inputs', () => {
    for (const quality of QUALITIES) {
      for (const viewport of [DESKTOP, TABLET, PHONE]) {
        const p = plan({ quality, viewport });
        expect(p.layers.map((entry) => entry.layer)).toEqual(['l0', 'l1', 'l2', 'l3']);
      }
    }
  });
});

describe('environment quality — §1.1 the parallax excursion ladder', () => {
  it('sets E to 12 px desktop, 8 px tablet, 0 px phone portrait', () => {
    expect(ENV_EXCURSION_PX).toEqual({ desktop: 12, tablet: 8, phonePortrait: 0 });
    expect(baseExcursionPx(DESKTOP)).toBe(12);
    expect(baseExcursionPx(TABLET)).toBe(8);
    expect(baseExcursionPx(PHONE)).toBe(0);
    expect(baseExcursionPx({ width: ENV_TABLET_MAX_WIDTH + 1, height: 800 })).toBe(12);
  });

  it('halves E at Standard, zeroes it at Lite, and zeroes it under reduced motion', () => {
    expect(plan({ quality: 'high' }).excursionPx).toBe(12);
    expect(plan({ quality: 'standard' }).excursionPx).toBe(6);
    expect(plan({ quality: 'lite' }).excursionPx).toBe(0);
    for (const quality of QUALITIES) {
      expect(plan({ quality, reducedMotion: true }).excursionPx).toBe(0);
    }
  });

  it('applies each layer’s §1 factor to E, and stays far below the 44 px hit floor', () => {
    const p = plan({ quality: 'high' });
    for (const id of ['l0', 'l1', 'l2', 'l3'] as EnvLayerId[]) {
      expect(layer(p, id).parallaxPx).toBeCloseTo(ENV_LAYERS[id].parallax * 12, 5);
      // The magnitude §1.1 chose so parallax can never move a hit target.
      expect(Math.abs(layer(p, id).parallaxPx)).toBeLessThan(44);
    }
    expect(layer(p, 'l3').parallaxPx).toBe(12);
  });
});

describe('environment quality — §7.1 ambient motion', () => {
  it('runs L0 + L3 at High, L0 halved at Standard, and nothing at Lite', () => {
    expect(ambientLevel('high', false)).toBe('l0+l3');
    expect(ambientLevel('standard', false)).toBe('l0-half');
    expect(ambientLevel('lite', false)).toBe('off');
  });

  it('turns ambient motion OFF at every level under reduced motion, including High', () => {
    // The §7.1 table's last row is absolute — reduced motion is not a quality
    // level, it is an accessibility request.
    for (const quality of QUALITIES) {
      expect(ambientLevel(quality, true)).toBe('off');
      expect(plan({ quality, reducedMotion: true }).ambient).toBe('off');
    }
  });
});

describe('environment quality — §7.2 the passive reaction hooks', () => {
  it('permits all five at High', () => {
    expect(hooksFor('high', false)).toEqual([...ENV_HOOKS]);
    expect(ENV_HOOKS).toHaveLength(5);
  });

  it('suppresses env.impact-ripple at Standard and every hook at Lite', () => {
    expect(hooksFor('standard', false)).not.toContain('env.impact-ripple');
    expect(hooksFor('standard', false)).toHaveLength(4);
    expect(hooksFor('lite', false)).toEqual([]);
  });

  it('suppresses every hook under reduced motion, at any level', () => {
    for (const quality of QUALITIES) {
      expect(hooksFor(quality, true)).toEqual([]);
    }
  });
});

describe('environment quality — §4.5 the phone-portrait recomposition', () => {
  it('recognises portrait by orientation, not by width', () => {
    expect(isPortraitViewport(PHONE)).toBe(true);
    expect(isPortraitViewport(TABLET)).toBe(false);
    expect(isPortraitViewport({ width: 800, height: 1200 })).toBe(true);
  });

  it('drops L0 to the token gradient, L2 to its lips, and L3 off', () => {
    const p = plan({ viewport: PHONE, quality: 'high' });
    expect(p.portrait).toBe(true);
    expect(layer(p, 'l0').treatment).toBe('token-gradient');
    expect(layer(p, 'l1').treatment).toBe('plate');
    expect(layer(p, 'l2').treatment).toBe('lips-only');
    expect(layer(p, 'l3').treatment).toBe('off');
    // §4.5: "E = 0, so no parallax."
    expect(p.excursionPx).toBe(0);
    for (const entry of p.layers) expect(entry.parallaxPx).toBe(0);
  });

  it('simplifies rather than shrinks — the composition is never a stretched crop', () => {
    // Portrait keeps L1 at full identity while every optional layer steps back;
    // this is what makes it a recomposition rather than a degraded landscape.
    const p = plan({ viewport: PHONE, quality: 'lite' });
    expect(layer(p, 'l1').treatment).toBe('plate');
    expect(layer(p, 'l1').variant).toBe('l1-half');
  });
});

describe('environment quality — §8.3 failure', () => {
  it('falls one failed layer back to its T0 form and leaves every sibling resolved', () => {
    const failed: EnvManifestKey[] = ['env/runicVale/l2'];
    const p = plan({ quality: 'high', failedKeys: failed });
    expect(layer(p, 'l2').treatment).toBe('off');
    expect(layer(p, 'l2').degraded).toBe(true);
    // Every other layer keeps its resolved form — one missing plate never cascades.
    for (const id of ['l0', 'l1', 'l3'] as EnvLayerId[]) {
      expect(layer(p, id).treatment).toBe('plate');
      expect(layer(p, id).degraded).toBe(false);
    }
  });

  it('falls a failed L1 back to the T0 plaza composition — the theme still reads', () => {
    const p = plan({ quality: 'high', failedKeys: ['env/runicVale/l1'] });
    expect(layer(p, 'l1').treatment).toBe('token-gradient');
    expect(layer(p, 'l1').degraded).toBe(true);
    expect(layer(p, 'l0').treatment).toBe('plate');
  });

  it('falls a whole failed theme back to runicVale at T0, never to a dark gradient', () => {
    // §8.3: "fall back to `runicVale` at T0, not to a dark dashboard gradient
    // (#542), and surface nothing modal."
    const every = ENV_VARIANTS.map((v) => `env/moonlitRuins/${v}` as EnvManifestKey);
    const p = plan({ theme: 'moonlitRuins', quality: 'high', failedKeys: every });
    expect(p.themeFellBack).toBe(true);
    expect(p.theme).toBe(DEFAULT_SCENE_THEME);
    // T0 is the surround gradient plus the plaza composition — an illustrated
    // fallback, not a hole and not a flat field.
    expect(layer(p, 'l0').treatment).toBe('token-gradient');
    expect(layer(p, 'l1').treatment).toBe('token-gradient');
  });

  it('does not fall the DEFAULT theme back to itself when its own keys fail', () => {
    const every = ENV_VARIANTS.map((v) => `env/runicVale/${v}` as EnvManifestKey);
    const p = plan({ quality: 'high', failedKeys: every });
    expect(p.themeFellBack).toBe(false);
    expect(p.theme).toBe(DEFAULT_SCENE_THEME);
    // It still renders: T0 is the terminal fallback and always draws something.
    expect(layer(p, 'l0').treatment).toBe('token-gradient');
    expect(layer(p, 'l1').treatment).toBe('token-gradient');
  });

  it('never yields an empty environment, for any combination of inputs', () => {
    // The §8.3 guarantee, exhaustively: theme × quality × motion × viewport ×
    // (nothing failed | everything failed).
    for (const theme of Object.keys(SCENE_THEMES) as SceneThemeName[]) {
      const allKeys = ENV_VARIANTS.map((v) => `env/${theme}/${v}` as EnvManifestKey);
      for (const failedKeys of [[], allKeys]) {
        for (const quality of QUALITIES) {
          for (const reducedMotion of [false, true]) {
            for (const viewport of [DESKTOP, TABLET, PHONE]) {
              const p = planEnvironment({ theme, quality, reducedMotion, viewport, failedKeys });
              expect(p.layers).toHaveLength(4);
              const drawing = p.layers.filter((entry) => entry.treatment !== 'off');
              expect(drawing.length).toBeGreaterThan(0);
              // L1 in particular always draws something.
              expect(layer(p, 'l1').treatment).not.toBe('off');
            }
          }
        }
      }
    }
  });
});

describe('environment quality — §8.2 T0 / T1 / T2 and the raster swap', () => {
  it('resolves every drawing layer through a stable manifest key', () => {
    const p = plan({ quality: 'high' });
    expect(layer(p, 'l0').key).toBe('env/runicVale/l0');
    expect(layer(p, 'l1').key).toBe('env/runicVale/l1');
    expect(layer(p, 'l2').key).toBe('env/runicVale/l2');
    expect(layer(p, 'l3').key).toBe('env/runicVale/l3');
    expect(layer(plan({ quality: 'lite' }), 'l1').key).toBe('env/runicVale/l1-half');
  });

  it('resolves the default theme to plates at T2, one per drawing layer', () => {
    // T2: #548's plates landed in #555, so every drawing layer of the default
    // theme now carries a URL. The keys above are unchanged, which is the claim
    // §10.5 makes — the swap moved pixels, not slots.
    for (const id of ['l0', 'l1', 'l2', 'l3'] as const) {
      const entry = layer(plan({ quality: 'high' }), id);
      expect(entry.rasterPath).toBe(ENV_MANIFESTS.runicVale.assets[entry.variant!].src);
    }
  });

  it('selects the tier’s variant, so Lite fetches the half-resolution L1', () => {
    // §8.1's quality matrix, now that the variants resolve to different files:
    // High and Standard draw the 1× plate, Lite the 0.5× one, and Lite's L0/L2/
    // L3 fetch nothing at all.
    const high = layer(plan({ quality: 'high' }), 'l1');
    const lite = layer(plan({ quality: 'lite' }), 'l1');
    expect(high.rasterPath).toBe(ENV_MANIFESTS.runicVale.assets.l1.src);
    expect(lite.rasterPath).toBe(ENV_MANIFESTS.runicVale.assets['l1-half'].src);
    expect(high.rasterPath).not.toBe(lite.rasterPath);
    for (const id of ['l0', 'l2', 'l3'] as const) {
      expect(layer(plan({ quality: 'lite' }), id).rasterPath).toBeUndefined();
    }
  });

  it('falls a failed plate back to its procedural form and fetches nothing more', () => {
    // §8.3, now reachable: a plate that 404s or fails to decode drops its own
    // layer to the T0 token treatment, keeps every sibling's plate, and never
    // retries — the URL is simply gone from the plan.
    const failed: EnvManifestKey[] = ['env/runicVale/l1'];
    const p = plan({ quality: 'high', failedKeys: failed });
    expect(layer(p, 'l1').rasterPath).toBeUndefined();
    expect(layer(p, 'l1').treatment).toBe('token-gradient');
    expect(layer(p, 'l1').degraded).toBe(true);
    expect(layer(p, 'l0').rasterPath).toBe(ENV_MANIFESTS.runicVale.assets.l0.src);
    expect(layer(p, 'l0').degraded).toBe(false);
    expect(layer(p, 'l2').rasterPath).toBe(ENV_MANIFESTS.runicVale.assets.l2.src);
  });

  it('never hands a plate to the portrait lip recomposition (§4.5)', () => {
    // The lips are re-anchored to canvas top and bottom rather than to source
    // coordinates, which a plate with its lips baked in at 21:9 cannot express.
    // Portrait L2 therefore always takes the procedural form.
    const p = plan({ quality: 'high', viewport: PHONE });
    expect(layer(p, 'l2').treatment).toBe('lips-only');
    expect(layer(p, 'l2').rasterPath).toBeUndefined();
    // L1 still takes its plate: `cover` fits it to the canvas width at scale ≥ 1
    // with the medallion pinned, which is exactly what §4.5 asks of L1.
    expect(layer(p, 'l1').rasterPath).toBe(ENV_MANIFESTS.runicVale.assets.l1.src);
  });

  it('attaches the sprite atlas to L3 only, and only when L3 draws raster', () => {
    expect(layer(plan({ quality: 'high' }), 'l3').atlas?.src).toBe(
      ENV_MANIFESTS.runicVale.assets.l3.src,
    );
    for (const id of ['l0', 'l1', 'l2'] as const) {
      expect(layer(plan({ quality: 'high' }), id).atlas).toBeUndefined();
    }
    expect(
      layer(plan({ quality: 'high', failedKeys: ['env/runicVale/l3'] }), 'l3').atlas,
    ).toBeUndefined();
  });

  it('is a pure function of its inputs — the same inputs always plan the same frame', () => {
    // §7 rule 3: "The environment carries no state that survives a view… a
    // reconnect renders it identically with animation suppressed."
    const a = plan({ quality: 'high', viewport: DESKTOP });
    const b = plan({ quality: 'high', viewport: DESKTOP });
    expect(a.layers).toEqual(b.layers);
    expect(a.ambient).toBe(b.ambient);
    expect(a.excursionPx).toBe(b.excursionPx);
    // …and the reconnect frame differs only by the motion collapse.
    const reconnect = plan({ quality: 'high', viewport: DESKTOP, reducedMotion: true });
    expect(reconnect.layers.map((l) => l.treatment)).toEqual(a.layers.map((l) => l.treatment));
    expect(reconnect.ambient).toBe('off');
  });
});

describe('environment quality — the completed theme family (#559)', () => {
  const THEMES: SceneThemeName[] = [
    'runicVale',
    'verdantCanals',
    'sunlitObservatory',
    'moonlitRuins',
  ];

  it('draws every theme as the complete layered contract', () => {
    for (const theme of THEMES) {
      const p = plan({ theme, quality: 'high' });
      expect(p.composition).toBe('layered');
      expect(p.composedActive).toBe(false);
      for (const id of ['l0', 'l1', 'l2', 'l3'] as const) {
        expect(layer(p, id).treatment).toBe('plate');
        expect(layer(p, id).rasterPath).toBeDefined();
      }
    }
  });

  it('gives every alternate a true raster Lite identity floor', () => {
    for (const theme of THEMES) {
      const p = plan({ theme, quality: 'lite' });
      expect(p.composedActive).toBe(false);
      expect(layer(p, 'l1').treatment).toBe('plate');
      expect(layer(p, 'l1').rasterPath).toBe(ENV_MANIFESTS[theme].assets['l1-half'].src);
      expect(layer(p, 'l1').variant).toBe('l1-half');
    }
  });

  it('falls back one failed layer without touching its three siblings', () => {
    for (const theme of THEMES) {
      const p = plan({ theme, quality: 'high', failedKeys: [`env/${theme}/l1`] });
      expect(p.composedActive).toBe(false);
      expect(layer(p, 'l1').treatment).toBe('token-gradient');
      expect(layer(p, 'l0').treatment).toBe('plate');
      expect(layer(p, 'l2').treatment).toBe('plate');
      expect(layer(p, 'l3').treatment).toBe('plate');
      expect(layer(p, 'l2').rasterPath).toBeDefined();
    }
  });

  it('switches themes without re-keying a slot or changing the layer count', () => {
    // §11's mid-match change: the manifest re-resolves and the layers cross-fade.
    // Nothing about the slot identity may move, or the cross-fade would be a
    // re-mount.
    for (const theme of THEMES) {
      const p = plan({ theme, quality: 'high' });
      expect(p.layers.map((l) => l.layer)).toEqual(['l0', 'l1', 'l2', 'l3']);
      expect(layer(p, 'l1').key).toBe(`env/${theme}/l1`);
      expect(p.theme).toBe(theme);
      expect(p.themeFellBack).toBe(false);
    }
  });
});
