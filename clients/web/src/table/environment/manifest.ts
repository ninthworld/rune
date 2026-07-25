/**
 * The environment layer contract and per-theme manifest
 * (`docs/design/environment-system.md` §1, §4, §9, §10) — issue #530.
 *
 * This module is the **slot identity** the production raster plates of issue
 * #548 will drop into. Every manifest key here resolves today to a procedural
 * renderer and tomorrow to a URL; §10.5's swap procedure is, in full:
 *
 * 1. Drop the plate files at the `path` of §9.1.
 * 2. Add ledger entries (`src/assets/ledger.json` + `ASSETS.md`).
 * 3. Flip that entry's `source` from `'procedural'` to `'raster'`.
 * 4. Done — no component change, no CSS change, no layout change, no test
 *    change. The §2.5 geometry tests read this manifest, not the art, so they
 *    keep passing unmodified and prove the swap preserved the contract.
 *
 * The placeholder renderer stays in the tree after the swap: it is the T0 and
 * per-layer failure fallback of §8.3 and the Lite L0 treatment, both permanent.
 */
import { SCENE_THEMES, type SceneThemeName } from '../../sceneTokens';
import {
  ENV_AMBIENT_SPACE,
  propRect,
  type EnvPropAnchor,
  type EnvPropMass,
  type FractionRect,
} from './zones';

// ── §1 The layer contract ────────────────────────────────────────────────────

/** The four layers of §1, back to front. Z-order is fixed and may not change. */
export const ENV_LAYER_IDS = ['l0', 'l1', 'l2', 'l3'] as const;

/** One of the four environment layers. */
export type EnvLayerId = (typeof ENV_LAYER_IDS)[number];

/** One layer's fixed properties: parallax factor and its §1 local-contrast cap. */
export interface EnvLayerSpec {
  /** Human name, as §1 states it. */
  label: string;
  /** Parallax factor applied to the environment's maximum excursion `E` (§1.1). */
  parallax: number;
  /** The layer's local-contrast ceiling, as a WCAG ratio (§1). */
  contrastCap: number;
  /** Z-index within the environment stack; L0 is furthest back. */
  depth: number;
}

/**
 * The §1 table as data. Parallax factors and contrast ceilings are
 * theme-invariant (§5.1) — a theme varies hue, prop identity, and material,
 * never these.
 */
export const ENV_LAYERS = {
  l0: { label: 'far surround', parallax: 0.15, contrastCap: 1.6, depth: 0 },
  l1: { label: 'arena floor', parallax: 0.35, contrastCap: 1.25, depth: 1 },
  l2: { label: 'arena edge', parallax: 0.6, contrastCap: 2.0, depth: 2 },
  l3: { label: 'props', parallax: 1.0, contrastCap: 2.6, depth: 3 },
} as const satisfies Record<EnvLayerId, EnvLayerSpec>;

/**
 * The authoring canvas of §4.1: one continuous 21:9 plate for L0–L2, with the
 * 16:9 safe crop marked. The placeholder SVG uses the same `viewBox` so the crop
 * code path is identical for both forms (§10.2).
 */
export const ENV_VIEWBOX = { width: 2333, height: 1000 } as const;

/** The authoring aspect, `2333 / 1000` (§4.1). */
export const ENV_AUTHORING_ASPECT = ENV_VIEWBOX.width / ENV_VIEWBOX.height;

// ── §9 Manifest keys, load classes, and the byte ledger ──────────────────────

/** The five asset variants one theme ships (§9.1). */
export const ENV_VARIANTS = ['l0', 'l1-half', 'l1', 'l2', 'l3'] as const;

/** One theme's asset variant — the suffix of a manifest key. */
export type EnvVariant = (typeof ENV_VARIANTS)[number];

/** A manifest key: `env/<theme>/<variant>`. Stable across the raster swap. */
export type EnvManifestKey = `env/${SceneThemeName}/${EnvVariant}`;

/**
 * Which load class a variant belongs to (§9.4). `first-match` ships in the
 * `dist/` root and counts against the ≤ 4 MB first-match budget; `lazy` uses the
 * `lazy/` prefix the load-budget gate classifies as deferred, which is the
 * explicit act that keeps a variant out of the first-match set.
 */
export type EnvLoadClass = 'first-match' | 'lazy';

/**
 * Where a manifest key's pixels come from **today**. `procedural` resolves to
 * the inline-SVG placeholder of §10; `raster` resolves to `path`. This is the
 * single field the §10.5 swap flips.
 */
export type EnvAssetSource = 'procedural' | 'raster';

/** One manifest entry: the slot a plate will occupy, and what fills it now. */
export interface EnvLayerAsset {
  /** The manifest key — the stable identity across the swap. */
  key: EnvManifestKey;
  /** Which of the four layers this variant draws. */
  layer: EnvLayerId;
  /** The variant's byte budget from the §9.1 ledger. */
  budgetBytes: number;
  /** The load class §9.4 assigns. */
  loadClass: EnvLoadClass;
  /** What fills the slot today. Flip to `'raster'` when the plate lands. */
  source: EnvAssetSource;
  /**
   * The path the plate ships at, relative to the built `dist/` root. Recorded
   * now so the swap is a file drop rather than a naming decision; unused while
   * `source` is `'procedural'`, which is why no request is ever issued.
   */
  path: string;
}

/** One L3 prop's manifest entry (§4.4) — props are anchored sprites, not a plate. */
export interface EnvPropEntry {
  /** Stable key, unique within the theme; addressable so §6.2 can hide exactly one. */
  key: string;
  /** Which of the six §4.4 anchors the prop hangs from. */
  anchor: EnvPropAnchor;
  /** Inboard offset from the anchor corner, in canvas fractions. */
  offset: { x: number; y: number };
  /** The prop's footprint, in canvas fractions. */
  size: { w: number; h: number };
  /** Mass class: `full` is Zone C only, `low` may also sit in Zone B. */
  mass: EnvPropMass;
  /** Which palette slot the silhouette draws in. */
  tone: 'warm' | 'cool';
  /** Whether this is the one prop §6.2 permits inside the ambient reservation. */
  ambient?: boolean;
}

/** One theme's complete manifest. */
export interface EnvThemeManifest {
  /** The theme key. */
  theme: SceneThemeName;
  /** Display label, mirrored from the token set. */
  label: string;
  /** The five asset variants, keyed by variant. */
  assets: Record<EnvVariant, EnvLayerAsset>;
  /** The theme's L3 props, on the §4.4 anchors. */
  props: readonly EnvPropEntry[];
}

/** The §9.1 per-variant byte budgets, in bytes (decimal kB, as the doc states them). */
const VARIANT_BUDGETS: Record<EnvVariant, number> = {
  l0: 290_000,
  'l1-half': 150_000,
  l1: 600_000,
  l2: 250_000,
  l3: 130_000,
};

/** Which layer each variant draws. */
const VARIANT_LAYER: Record<EnvVariant, EnvLayerId> = {
  l0: 'l0',
  'l1-half': 'l1',
  l1: 'l1',
  l2: 'l2',
  l3: 'l3',
};

/**
 * §9.1's first-match set: L0, the half-resolution L1, and the manifest. The 1×
 * L1, L2, and L3 are `lazy/` upgrades that arrive after the match is
 * interactive — which is what lets "lobby → match presentation ready ≤ 2 s" be
 * met by the T0/T1 path without ever waiting on a plate.
 */
const FIRST_MATCH_VARIANTS: readonly EnvVariant[] = ['l0', 'l1-half'];

/**
 * The per-theme compressed ceiling of ADR 0031 and §9.1, in bytes. Counts
 * **all** load classes for that theme.
 */
export const ENV_THEME_BUDGET_BYTES = 1_500_000;

/**
 * The §10.4 cap on the placeholder renderer: ≤ 12 KB gzipped for all four layer
 * components combined, across all themes — themes are data, the renderer is one.
 * It counts against the interactive code bundle, never against an asset budget,
 * so the per-theme 1.5 MB line reads **0 KB used** until the first plate lands.
 */
export const ENV_PLACEHOLDER_CODE_CAP_BYTES = 12_000;

/**
 * Build one theme's asset table. Every non-default theme is entirely `lazy/`
 * (§11): selecting one downloads it once, content-hashed and cached forever,
 * and the match stays playable throughout.
 */
function assetsFor(theme: SceneThemeName, isDefault: boolean): Record<EnvVariant, EnvLayerAsset> {
  const entries = ENV_VARIANTS.map((variant): [EnvVariant, EnvLayerAsset] => {
    const loadClass: EnvLoadClass =
      isDefault && FIRST_MATCH_VARIANTS.includes(variant) ? 'first-match' : 'lazy';
    const prefix = loadClass === 'lazy' ? 'lazy/' : '';
    return [
      variant,
      {
        key: `env/${theme}/${variant}`,
        layer: VARIANT_LAYER[variant],
        budgetBytes: VARIANT_BUDGETS[variant],
        loadClass,
        // No production plate exists yet (#548). Every key resolves to the
        // inline-SVG placeholder until a file lands at `path`.
        source: 'procedural',
        path: `${prefix}env/${theme}/${variant}.avif`,
      },
    ];
  });
  return Object.fromEntries(entries) as Record<EnvVariant, EnvLayerAsset>;
}

/**
 * The prop vocabulary, shared in **placement** by every theme and varying only
 * in identity (§5.1: "a theme may leave one anchor empty, never add one"). The
 * placeholder draws two warm lantern silhouettes and two cool crystal plinths on
 * exactly these anchors (§10.3), which are the anchors the plate's props will
 * use — so the swap moves pixels, not geometry.
 *
 * `bottom-left` carries the `ambient` mark: it is the single prop §6.2 permits
 * inside the `AMBIENT SPACE — FUTURE` reservation, independently addressable so
 * claiming the region hides exactly one thing.
 */
const SHARED_PROPS: readonly EnvPropEntry[] = [
  {
    key: 'lantern-top-left',
    anchor: 'top-left',
    offset: { x: 0.012, y: 0.008 },
    size: { w: 0.055, h: 0.085 },
    mass: 'full',
    tone: 'warm',
  },
  {
    key: 'crystal-top-right',
    anchor: 'top-right',
    offset: { x: 0.012, y: 0.008 },
    size: { w: 0.06, h: 0.085 },
    mass: 'full',
    tone: 'cool',
  },
  {
    key: 'lantern-bottom-left',
    anchor: 'bottom-left',
    offset: { x: 0.014, y: 0.03 },
    size: { w: 0.05, h: 0.11 },
    mass: 'full',
    tone: 'warm',
    ambient: true,
  },
  {
    key: 'crystal-bottom-right',
    anchor: 'bottom-right',
    offset: { x: 0.014, y: 0.03 },
    size: { w: 0.05, h: 0.11 },
    mass: 'full',
    tone: 'cool',
  },
  // The pinched mid anchors §4.4 limits to low-mass ground cover: the baseline's
  // crystal plinths, squeezed into the outer ~3 % of width at mid height.
  {
    key: 'plinth-left-mid',
    anchor: 'left-mid',
    offset: { x: 0.004, y: 0.3 },
    size: { w: 0.028, h: 0.13 },
    mass: 'low',
    tone: 'cool',
  },
  {
    key: 'plinth-right-mid',
    anchor: 'right-mid',
    offset: { x: 0.004, y: 0.3 },
    size: { w: 0.028, h: 0.13 },
    mass: 'low',
    tone: 'cool',
  },
];

/** Every theme's manifest, keyed by theme. */
export const ENV_MANIFESTS: Record<SceneThemeName, EnvThemeManifest> = Object.fromEntries(
  (Object.keys(SCENE_THEMES) as SceneThemeName[]).map((theme) => [
    theme,
    {
      theme,
      label: SCENE_THEMES[theme].label,
      assets: assetsFor(theme, theme === 'runicVale'),
      props: SHARED_PROPS,
    } satisfies EnvThemeManifest,
  ]),
) as Record<SceneThemeName, EnvThemeManifest>;

/** A prop entry's footprint on the composed canvas (§4.4). */
export function propFootprint(prop: EnvPropEntry): FractionRect {
  return propRect(prop.anchor, prop.offset, prop.size);
}

/** The props a theme anchors inside the §6 ambient reservation. */
export function ambientProps(manifest: EnvThemeManifest): readonly EnvPropEntry[] {
  return manifest.props.filter((prop) => prop.ambient === true);
}

/**
 * The theme's total committed asset bytes today. Zero until the first plate
 * lands: the placeholder is code, so the §9.1 ledger reads 0 KB of 1.5 MB and
 * the ADR 0031 provenance gate has nothing to gate (§12 conflict 9).
 */
export function themeAssetBytes(manifest: EnvThemeManifest): number {
  return Object.values(manifest.assets)
    .filter((asset) => asset.source === 'raster')
    .reduce((sum, asset) => sum + asset.budgetBytes, 0);
}

/** The budget a theme would consume once every plate lands (the §9.1 total). */
export function themeBudgetedBytes(manifest: EnvThemeManifest): number {
  return Object.values(manifest.assets).reduce((sum, asset) => sum + asset.budgetBytes, 0);
}

/** Whether the ambient reservation stays within its §6.2 one-prop density rule. */
export function ambientReservationIsQuiet(manifest: EnvThemeManifest): boolean {
  const inside = manifest.props.filter((prop) => {
    const rect = propFootprint(prop);
    return (
      rect.x < ENV_AMBIENT_SPACE.rect.x + ENV_AMBIENT_SPACE.rect.w &&
      rect.y + rect.h > ENV_AMBIENT_SPACE.rect.y
    );
  });
  return inside.length <= 1 && inside.every((prop) => prop.ambient === true);
}
