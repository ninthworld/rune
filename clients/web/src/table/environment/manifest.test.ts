/**
 * The layer-contract, manifest, and aspect gates of
 * `docs/design/environment-system.md` §1, §4, §9, and §10 — issue #530.
 *
 * The single question this file answers: **when the production raster plates of
 * issue #548 arrive, is the drop-in a file swap plus a ledger entry, or a
 * rework?** §10.2 lists the properties the placeholder must share with the
 * future plate for the answer to be "a swap", and each is pinned below.
 *
 * These tests read the MANIFEST, not the art, which is why §10.5 step 4 can
 * promise "no test change": flipping an entry's `source` to `'raster'` leaves
 * every assertion here true.
 */
import { statSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import {
  PRODUCTION_ENVIRONMENTS,
  PRODUCTION_ENVIRONMENT_STUDIES,
} from '../../assets/productionManifest';
import { SCENE_THEMES, type SceneThemeName } from '../../sceneTokens';
import { ENV_PROP_ANCHORS, propPlacementIsLegal } from './zones';
import {
  ENV_AUTHORING_ASPECT,
  ENV_LAYERS,
  ENV_LAYER_IDS,
  ENV_MANIFESTS,
  ENV_PLACEHOLDER_CODE_CAP_BYTES,
  ENV_THEME_BUDGET_BYTES,
  ENV_VARIANTS,
  ENV_VIEWBOX,
  propFootprint,
  themeAssetBytes,
  themeBudgetedBytes,
  type EnvLayerId,
  type EnvThemeManifest,
} from './manifest';
import {
  ENV_PORTRAIT_ASPECT_CEILING,
  ENV_TIGHTEST_ASPECT,
  cropForAspect,
  cropForViewport,
} from './crop';

const THEMES = Object.values(ENV_MANIFESTS) as EnvThemeManifest[];

describe('environment layers — §1 the layer contract', () => {
  it('ships exactly four layers, back to front, in fixed z-order', () => {
    // "No layer may be reordered, merged, or given a fifth sibling without
    // amending this document."
    expect(ENV_LAYER_IDS).toEqual(['l0', 'l1', 'l2', 'l3']);
    const depths = ENV_LAYER_IDS.map((id) => ENV_LAYERS[id].depth);
    expect(depths).toEqual([0, 1, 2, 3]);
  });

  it('carries the §1 parallax factors verbatim', () => {
    expect(ENV_LAYERS.l0.parallax).toBe(0.15);
    expect(ENV_LAYERS.l1.parallax).toBe(0.35);
    expect(ENV_LAYERS.l2.parallax).toBe(0.6);
    expect(ENV_LAYERS.l3.parallax).toBe(1.0);
    // Strictly increasing with depth — the whole point of a parallax ladder.
    const factors = ENV_LAYER_IDS.map((id) => ENV_LAYERS[id].parallax);
    expect([...factors].sort((a, b) => a - b)).toEqual(factors);
  });

  it('carries the §1 per-layer local-contrast ceilings, rising toward the viewer', () => {
    expect(ENV_LAYERS.l0.contrastCap).toBe(1.6);
    expect(ENV_LAYERS.l1.contrastCap).toBe(1.25);
    expect(ENV_LAYERS.l2.contrastCap).toBe(2.0);
    expect(ENV_LAYERS.l3.contrastCap).toBe(2.6);
    // L1's cap is the tightest of all: it is the layer cards sit directly on.
    const caps = ENV_LAYER_IDS.map((id) => ENV_LAYERS[id].contrastCap);
    expect(Math.min(...caps)).toBe(ENV_LAYERS.l1.contrastCap);
  });
});

describe('environment manifest — §10.2 the slot identity the plate inherits', () => {
  it('gives every theme the same five variants under stable `env/<theme>/<variant>` keys', () => {
    expect(ENV_VARIANTS).toEqual(['l0', 'l1-half', 'l1', 'l2', 'l3']);
    for (const manifest of THEMES) {
      expect(Object.keys(manifest.assets).sort()).toEqual([...ENV_VARIANTS].sort());
      for (const variant of ENV_VARIANTS) {
        const asset = manifest.assets[variant];
        expect(asset.key).toBe(`env/${manifest.theme}/${variant}`);
      }
    }
  });

  it('covers every shipped theme, labelled from the token set', () => {
    const themes = Object.keys(ENV_MANIFESTS).sort() as SceneThemeName[];
    expect(themes).toEqual((Object.keys(SCENE_THEMES) as SceneThemeName[]).sort());
    for (const manifest of THEMES) {
      expect(manifest.label).toBe(SCENE_THEMES[manifest.theme].label);
    }
  });

  it('maps every variant onto one of the four layers', () => {
    const byLayer = new Map<EnvLayerId, number>();
    for (const variant of ENV_VARIANTS) {
      const layer = ENV_MANIFESTS.runicVale.assets[variant].layer;
      expect(ENV_LAYER_IDS).toContain(layer);
      byLayer.set(layer, (byLayer.get(layer) ?? 0) + 1);
    }
    // L1 is the only layer with two variants: §9.1 ships it twice so Lite keeps
    // the theme's identity without the bytes.
    expect(byLayer.get('l1')).toBe(2);
    expect(byLayer.get('l0')).toBe(1);
    expect(byLayer.get('l2')).toBe(1);
    expect(byLayer.get('l3')).toBe(1);
  });

  it('authors L0–L2 on the 21:9 canvas §4.1 and §10.2 both name', () => {
    expect(ENV_VIEWBOX).toEqual({ width: 2333, height: 1000 });
    expect(ENV_AUTHORING_ASPECT).toBeCloseTo(2.333, 3);
  });

  it('resolves Runic Vale’s five keys to the plates #555 shipped (§10.5 step 3)', () => {
    // The swap itself. Every variant of the default theme now resolves to a
    // content-hashed URL read from `public/assets/manifest.json`; the keys, the
    // layer mapping, the load classes, and the geometry above are untouched,
    // which is the whole claim §10.5 makes about the drop.
    for (const variant of ENV_VARIANTS) {
      const asset = ENV_MANIFESTS.runicVale.assets[variant];
      expect(asset.source).toBe('raster');
      expect(asset.src).toMatch(/^\/(?:assets|lazy)\/environments\/runic-vale\/.+\.webp$/);
      expect(asset.pixels?.width).toBeGreaterThan(0);
    }
  });

  it('gives the three studies one composed plate and leaves the rest procedural', () => {
    // A study is key art, not a layer set (#555 shipped one flattened image per
    // theme). It fills the identity floor and nothing else, so L0, L2, L3, and
    // the half-resolution Lite variant stay on the §10 placeholder.
    for (const manifest of THEMES) {
      if (manifest.theme === 'runicVale') continue;
      expect(manifest.composition).toBe('composed');
      expect(manifest.assets.l1.source).toBe('raster');
      expect(manifest.assets.l1.src).toMatch(/^\/lazy\/environment-studies\/.+\.webp$/);
      for (const variant of ['l0', 'l1-half', 'l2', 'l3'] as const) {
        expect(manifest.assets[variant].source).toBe('procedural');
        expect(manifest.assets[variant].src).toBeUndefined();
      }
      expect(manifest.atlas).toBeUndefined();
    }
    expect(ENV_MANIFESTS.runicVale.composition).toBe('layered');
  });

  it('never transcribes a content hash — every URL is read from the shipped manifest', () => {
    // A hash written into TypeScript is a future breakage the type system
    // cannot catch. Every `src` here must be the exact string the generator
    // wrote, so a regeneration can never leave the client pointing at a file
    // that no longer exists.
    const shipped = new Set<string>();
    for (const environment of Object.values(PRODUCTION_ENVIRONMENTS)) {
      for (const layer of Object.values(environment.layers)) shipped.add(layer.src);
    }
    for (const study of Object.values(PRODUCTION_ENVIRONMENT_STUDIES)) shipped.add(study.src);
    for (const manifest of THEMES) {
      for (const variant of ENV_VARIANTS) {
        const src = manifest.assets[variant].src;
        if (src !== undefined) expect(shipped.has(src)).toBe(true);
      }
      if (manifest.atlas) expect(shipped.has(manifest.atlas.src)).toBe(true);
    }
  });

  it('keeps the recorded §9.1 slot path, and never lets it disagree with the load class', () => {
    for (const manifest of THEMES) {
      for (const variant of ENV_VARIANTS) {
        const asset = manifest.assets[variant];
        expect(asset.path).toContain(`env/${manifest.theme}/${variant}`);
        // §9.4: the `lazy/` prefix IS the mechanism that keeps a variant out of
        // the first-match set, so the class and the path may never disagree —
        // and neither may the shipped URL, which carries the same prefix.
        expect(asset.path.startsWith('lazy/')).toBe(asset.loadClass === 'lazy');
        if (asset.src !== undefined) {
          expect(asset.src.startsWith('/lazy/')).toBe(asset.loadClass === 'lazy');
        }
      }
    }
  });
});

describe('environment L3 — §4.4 the shipped sprite atlas', () => {
  it('gives Runic Vale one atlas frame per §4.4 anchor, and no theme a seventh', () => {
    const manifest = ENV_MANIFESTS.runicVale;
    expect(manifest.atlas?.src).toBe(manifest.assets.l3.src);
    const anchors = manifest.props.filter((p) => p.frame).map((p) => p.anchor);
    expect([...anchors].sort()).toEqual([...ENV_PROP_ANCHORS].sort());
    expect(new Set(manifest.props.map((p) => p.frame?.key)).size).toBe(manifest.props.length);
  });

  it('keeps the manifest’s own placement, not the atlas’s — §2.2 is binding', () => {
    // The shipped frames record a `scale` of 0.17–0.24. At EVERY reading of that
    // number — canvas width or canvas height — the drawn sprite is wider than
    // the 10 % of canvas width Zone C allows, so honouring it would push L3 into
    // the focal core. The atlas supplies pixels; the manifest supplies geometry.
    const manifest = ENV_MANIFESTS.runicVale;
    for (const prop of manifest.props) {
      expect(propPlacementIsLegal(prop.anchor, propFootprint(prop), prop.mass)).toBe(true);
      expect(Math.max(prop.size.w, prop.size.h)).toBeLessThan(0.17);
    }
  });

  it('takes only rectangles from the atlas, and they all lie inside it', () => {
    const manifest = ENV_MANIFESTS.runicVale;
    const atlas = manifest.atlas!;
    for (const prop of manifest.props) {
      const frame = prop.frame!;
      expect(frame.x).toBeGreaterThanOrEqual(0);
      expect(frame.y).toBeGreaterThanOrEqual(0);
      expect(frame.x + frame.w).toBeLessThanOrEqual(atlas.width);
      expect(frame.y + frame.h).toBeLessThanOrEqual(atlas.height);
    }
  });
});

describe('environment manifest — §9 the asset budget', () => {
  it('keeps every theme’s full ledger inside the ≤ 1.5 MB per-theme ceiling', () => {
    expect(ENV_THEME_BUDGET_BYTES).toBe(1_500_000);
    for (const manifest of THEMES) {
      const budgeted = themeBudgetedBytes(manifest);
      expect(budgeted).toBe(1_420_000);
      expect(budgeted).toBeLessThanOrEqual(ENV_THEME_BUDGET_BYTES);
    }
  });

  it('claims the whole §9.1 allocation for Runic Vale and one slot for a study', () => {
    // The allocation the filled slots claim — not a measurement. The real bytes
    // are measured below, straight off disk.
    expect(themeAssetBytes(ENV_MANIFESTS.runicVale)).toBe(1_420_000);
    for (const manifest of THEMES) {
      if (manifest.theme === 'runicVale') continue;
      expect(themeAssetBytes(manifest)).toBe(600_000);
    }
  });

  it('measures the shipped bytes against the ≤ 1.5 MB per-theme ceiling', () => {
    // The binding ceiling of ADR 0031 / §9.1, measured rather than budgeted.
    // `scripts/checkAssetLedger.js` gates the same number in CI; this asserts it
    // against the manifest the client actually resolves, so a regeneration that
    // grew a plate fails here as well as there.
    const publicDir = resolve(dirname(fileURLToPath(import.meta.url)), '../../../public');
    const bytesOf = (src: string): number => statSync(resolve(publicDir, src.slice(1))).size;
    for (const manifest of THEMES) {
      const bytes = ENV_VARIANTS.map((v) => manifest.assets[v].src)
        .filter((src): src is string => src !== undefined)
        .reduce((sum, src) => sum + bytesOf(src), 0);
      expect(bytes).toBeLessThanOrEqual(ENV_THEME_BUDGET_BYTES);
    }
    // The default theme's first-match slice is what "lobby → match presentation
    // ready ≤ 2 s" is measured against; it must stay a small fraction of the
    // 4 MB first-match ceiling.
    const firstMatch = ENV_VARIANTS.filter(
      (v) => ENV_MANIFESTS.runicVale.assets[v].loadClass === 'first-match',
    )
      .map((v) => ENV_MANIFESTS.runicVale.assets[v].src!)
      .reduce((sum, src) => sum + bytesOf(src), 0);
    expect(firstMatch).toBeLessThan(500_000);
  });

  it('puts only the default theme’s L0 and half-res L1 in the first-match class', () => {
    // §9.2's cross-check: the first-match slice is 444 KB, and the 980 KB `lazy/`
    // upgrade arrives after the match is interactive, so "lobby → match
    // presentation ready ≤ 2 s" is met by the T0/T1 path and never waits on a
    // plate.
    const firstMatch = ENV_VARIANTS.filter(
      (v) => ENV_MANIFESTS.runicVale.assets[v].loadClass === 'first-match',
    );
    expect(firstMatch).toEqual(['l0', 'l1-half']);
    const bytes = firstMatch.reduce(
      (sum, v) => sum + ENV_MANIFESTS.runicVale.assets[v].budgetBytes,
      0,
    );
    expect(bytes).toBe(440_000);
    expect(bytes).toBeLessThan(4_000_000);
  });

  it('makes every non-default theme entirely lazy (§11)', () => {
    for (const manifest of THEMES) {
      if (manifest.theme === 'runicVale') continue;
      for (const variant of ENV_VARIANTS) {
        expect(manifest.assets[variant].loadClass).toBe('lazy');
      }
    }
  });

  it('keeps the whole four-theme set inside the ADR 0031 12 MB repository ceiling', () => {
    const total = THEMES.reduce((sum, manifest) => sum + themeBudgetedBytes(manifest), 0);
    expect(total).toBe(5_680_000);
    expect(total).toBeLessThan(12_000_000);
  });

  it('caps the placeholder renderer against the code bundle, not an asset budget', () => {
    expect(ENV_PLACEHOLDER_CODE_CAP_BYTES).toBe(12_000);
  });
});

describe('environment crops — §4 aspect handling', () => {
  it('uses the whole plate at 21:9 and reveals rather than stretches', () => {
    const wide = cropForAspect(ENV_AUTHORING_ASPECT);
    expect(wide.sourceWidthFraction).toBe(1);
    expect(wide.source).toEqual({ x: 0, y: 0, w: 1, h: 1 });
    // Beyond the authoring aspect the crop is still the whole plate — never more.
    expect(cropForAspect(3).sourceWidthFraction).toBe(1);
  });

  it('reproduces the §4.2 crop table', () => {
    const rows: [number, number][] = [
      [21 / 9, 1],
      [16 / 9, 0.762],
      [16 / 10, 0.686],
      [3 / 2, 0.643],
      [1180 / 820, 0.617],
      [4 / 3, 0.571],
    ];
    // The table states one decimal of a percentage, so 4:3's true 57.15 % is
    // printed as 57.1 %; a 0.1 % tolerance covers the document's own rounding.
    for (const [aspect, fraction] of rows) {
      expect(Math.abs(cropForAspect(aspect).sourceWidthFraction - fraction)).toBeLessThan(0.001);
    }
    // 16:9 reveals the outer 23.8 % on ultrawide — the §4.1 statement.
    expect(1 - cropForAspect(16 / 9).sourceWidthFraction).toBeCloseTo(0.238, 3);
  });

  it('centres every landscape crop, so no per-theme anchor table is needed', () => {
    for (const aspect of [21 / 9, 16 / 9, 16 / 10, 3 / 2, 1180 / 820, 4 / 3]) {
      const crop = cropForAspect(aspect);
      expect(crop.source.x).toBeCloseTo((1 - crop.sourceWidthFraction) / 2, 10);
      expect(crop.source.y).toBe(0);
      expect(crop.source.h).toBe(1);
      expect(crop.recomposed).toBe(false);
    }
  });

  it('names 4:3 as the tightest landscape crop, at source x ∈ [21.4 %, 78.6 %]', () => {
    expect(ENV_TIGHTEST_ASPECT).toBeCloseTo(4 / 3, 10);
    const crop = cropForAspect(ENV_TIGHTEST_ASPECT);
    expect(crop.source.x).toBeCloseTo(0.214, 3);
    expect(crop.source.x + crop.source.w).toBeCloseTo(0.786, 3);
  });

  it('marks phone portrait as a recomposition, never a crop (§4.5)', () => {
    // A 0.462 aspect would use 20 % of the plate's width and show nothing
    // recognisable, which is why portrait re-anchors instead of cropping.
    const portrait = cropForViewport({ width: 390, height: 844 });
    expect(portrait.recomposed).toBe(true);
    expect(portrait.sourceWidthFraction).toBe(1);
    expect(ENV_PORTRAIT_ASPECT_CEILING).toBe(1);
    expect(cropForViewport({ width: 1680, height: 945 }).recomposed).toBe(false);
  });

  it('emits a viewBox on the authoring canvas for every crop', () => {
    const crop = cropForAspect(16 / 9);
    const [x, y, w, h] = crop.viewBox.split(' ').map(Number);
    expect(y).toBe(0);
    expect(h).toBe(ENV_VIEWBOX.height);
    expect(x).toBeCloseTo(crop.source.x * ENV_VIEWBOX.width, 1);
    expect(w).toBeCloseTo(crop.sourceWidthFraction * ENV_VIEWBOX.width, 1);
  });

  it('guards a degenerate viewport rather than dividing by zero', () => {
    expect(cropForViewport({ width: 100, height: 0 }).sourceWidthFraction).toBe(1);
  });
});
