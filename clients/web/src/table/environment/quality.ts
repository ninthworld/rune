/**
 * The environment's quality, loading, and failure matrix
 * (`docs/design/environment-system.md` §1.1, §4.5, §7.1, §8) — issue #530.
 *
 * A pure resolver: `(theme, quality, reduced motion, viewport, failed keys)` →
 * the plan the mount renders. Nothing here fetches, measures, or remembers, so
 * the §7 noninteractive contract holds by construction — the environment is a
 * pure function of its inputs, and a reconnect renders it identically with
 * animation suppressed.
 *
 * The two rules that shape every branch below:
 *
 * - **The match is fully interactive at T0.** No state may move a hit target,
 *   change a rect, or block a scene build, so every degradation and every
 *   failure resolves to a *treatment*, never to an absent layer or a hole.
 * - **L1 is never dropped.** Lite keeps the theme's identity floor at half
 *   resolution rather than collapsing to a gradient (§8.1) — the requirement
 *   #542 set and the reason the L1 row has no `off`.
 */
import type { EffectQuality } from '../effects';
import { DEFAULT_SCENE_THEME, type SceneThemeName } from '../../sceneTokens';
import {
  ENV_LAYERS,
  ENV_MANIFESTS,
  type EnvComposition,
  type EnvLayerId,
  type EnvManifestKey,
  type EnvPropAtlas,
  type EnvThemeManifest,
  type EnvVariant,
} from './manifest';

/** The viewport the environment composes into (logical px). */
export interface EnvViewport {
  /** Logical width. */
  width: number;
  /** Logical height. */
  height: number;
}

/**
 * How one layer resolves for a frame:
 *
 * - `plate` — the layer's art renders (SVG placeholder today, raster after the
 *   §10.5 swap; the plan does not care which, that is the manifest's business).
 * - `token-gradient` — the T0 token composition stands in. Zero bytes, zero
 *   fetches. Lite's L0, and every failed layer's fallback (§8.3).
 * - `lips-only` — L2's phone-portrait recomposition: the two lips re-anchored
 *   to canvas top and bottom rather than to source coordinates (§4.5).
 * - `off` — the layer does not render at all.
 */
export type EnvLayerTreatment = 'plate' | 'token-gradient' | 'lips-only' | 'off';

/** One layer's resolved plan for a frame. */
export interface EnvLayerPlan {
  /** Which layer. */
  layer: EnvLayerId;
  /** How it renders. */
  treatment: EnvLayerTreatment;
  /** The manifest variant it draws, when it draws art. */
  variant?: EnvVariant;
  /** The manifest key the variant resolves through — stable across the swap. */
  key?: EnvManifestKey;
  /**
   * The plate URL, present only when the manifest entry resolved to
   * `source: 'raster'` (§10.5 step 3) **and** the layer is drawing a full plate.
   * Absent means the slot renders the procedural placeholder: no plate shipped
   * for it, the plate failed, or the treatment is one the plate cannot serve —
   * `lips-only`, where §4.5 re-anchors two bands the composed plate has baked in
   * at source coordinates.
   */
  rasterPath?: string;
  /**
   * The L3 sprite atlas, present only when this layer draws raster props. L3 is
   * not a plate (§4.4): the atlas is cropped per prop at the manifest's own
   * anchors, so 16:9 and 21:9 place a prop identically.
   */
  atlas?: EnvPropAtlas;
  /**
   * Whether this layer is drawing a **composed** study rather than its own
   * layer (see {@link EnvComposition}). Purely descriptive; the mount uses it
   * for provenance in the DOM.
   */
  composed?: boolean;
  /** The parallax offset in logical px at full excursion (§1.1). */
  parallaxPx: number;
  /** Whether this layer fell back because its key failed to resolve (§8.3). */
  degraded: boolean;
}

/** Ambient motion level (§7.1): which layers may breathe, and how far. */
export type EnvAmbient = 'l0+l3' | 'l0-half' | 'off';

/** The composed plan one frame of environment renders from. */
export interface EnvironmentPlan {
  /** The resolved theme — never an unknown key (§8.3 rewrites a stale one). */
  theme: SceneThemeName;
  /** The theme's manifest. */
  manifest: EnvThemeManifest;
  /** Whether the theme fell back to the default because its own set failed. */
  themeFellBack: boolean;
  /** The four layers, in z-order L0 → L3. */
  layers: EnvLayerPlan[];
  /** The environment's maximum excursion `E`, logical px (§1.1). */
  excursionPx: number;
  /** Ambient-motion level (§7.1). */
  ambient: EnvAmbient;
  /** Whether the phone-portrait recomposition engaged (§4.5). */
  portrait: boolean;
  /** The passive reaction hooks permitted at this tier (§7.2). */
  hooks: readonly EnvHook[];
  /** How the resolved theme's pixels shipped. */
  composition: EnvComposition;
  /**
   * Whether the frame is actually drawing a flattened study: a `composed` theme
   * whose single plate resolved. When it is, L2 and L3 stand down — the study
   * already carries its own edge and props, and drawing the procedural rim and
   * silhouettes over it would double them.
   */
  composedActive: boolean;
}

// ── §7.2 Passive reaction hooks ──────────────────────────────────────────────

/**
 * The five passive reaction hooks of §7.2, keyed to the effect taxonomy the
 * scene already derives — no new protocol channel and no new log event. Each is
 * cosmetic, has no hit target, changes no layout, and corroborates something the
 * crest, phase pill, log, or card treatment already states, so none is ever the
 * sole channel for information (§7 rule 5).
 *
 * The tier gate is implemented here; wiring the hooks to the presentation
 * stream is deliberately **not** part of this issue — see the ledger comment in
 * `SceneEnvironment.tsx`.
 */
export const ENV_HOOKS = [
  'env.priority-pulse',
  'env.turn-tint',
  'env.impact-ripple',
  'env.loss-dim',
  'env.victory-bloom',
] as const;

/** One passive environment reaction hook. */
export type EnvHook = (typeof ENV_HOOKS)[number];

/** The one hook §8.1 additionally suppresses at Standard. */
const STANDARD_SUPPRESSED: EnvHook = 'env.impact-ripple';

// ── §1.1 The excursion ladder ────────────────────────────────────────────────

/**
 * The maximum excursion `E` by device class (§1.1). The magnitude sits far below
 * the 44 px hit floor and below one card's contact-shadow spread, which is what
 * makes parallax unable to move anything a player aims at.
 */
export const ENV_EXCURSION_PX = { desktop: 12, tablet: 8, phonePortrait: 0 } as const;

/** The width at or below which a landscape viewport is treated as a tablet (§1.1). */
export const ENV_TABLET_MAX_WIDTH = 1180;

/**
 * How far the environment is displaced this frame, in `-1…1` per axis. Driven
 * **only** by the staging tween's plane delta (§1.1) — never by pointer
 * position, device orientation, or scroll, because ADR 0030 has no free camera.
 */
export interface EnvBias {
  /** Horizontal displacement, `-1` (left) … `1` (right). */
  x: number;
  /** Vertical displacement, `-1` (up) … `1` (down). */
  y: number;
}

/** Whether the viewport takes the §4.5 phone-portrait recomposition. */
export function isPortraitViewport(viewport: EnvViewport): boolean {
  return viewport.height > viewport.width;
}

/** The base excursion for a viewport, before the quality and motion collapses. */
export function baseExcursionPx(viewport: EnvViewport): number {
  if (isPortraitViewport(viewport)) return ENV_EXCURSION_PX.phonePortrait;
  return viewport.width <= ENV_TABLET_MAX_WIDTH
    ? ENV_EXCURSION_PX.tablet
    : ENV_EXCURSION_PX.desktop;
}

// ── The resolver ─────────────────────────────────────────────────────────────

/** Inputs the plan is a pure function of. */
export interface EnvironmentPlanInput {
  /** The requested theme; an unknown key is the caller's to guard (§8.3). */
  theme: SceneThemeName;
  /** The device-local quality level. */
  quality: EffectQuality;
  /** The composed `prefers-reduced-motion` result. */
  reducedMotion: boolean;
  /** The viewport the environment composes into. */
  viewport: EnvViewport;
  /**
   * Manifest keys that failed to resolve. A failed layer falls back to its T0
   * token treatment and **every other layer keeps its resolved form** (§8.3);
   * when every key of a theme has failed the whole theme falls back to the
   * default at T0, never to a dark dashboard gradient.
   */
  failedKeys?: Iterable<EnvManifestKey>;
}

/** Which variant a layer draws at a quality level (§8.1). */
function variantFor(layer: EnvLayerId, quality: EffectQuality): EnvVariant | undefined {
  if (layer === 'l0') return quality === 'lite' ? undefined : 'l0';
  if (layer === 'l1') return quality === 'lite' ? 'l1-half' : 'l1';
  if (layer === 'l2') return quality === 'lite' ? undefined : 'l2';
  return quality === 'lite' ? undefined : 'l3';
}

/** The §8.1 treatment for a layer, before failure and portrait are applied. */
function baseTreatment(layer: EnvLayerId, quality: EffectQuality): EnvLayerTreatment {
  if (quality !== 'lite') return 'plate';
  // Lite: L0 becomes the token gradient (no fetch), L2 and L3 go off, and L1
  // keeps the theme's identity at half resolution — L1 is never dropped.
  if (layer === 'l0') return 'token-gradient';
  if (layer === 'l1') return 'plate';
  return 'off';
}

/**
 * A layer's **T0** treatment (§8.2) — what it renders with no asset at all. The
 * T0 composition is the surround gradient plus the plaza ellipse, its medallion,
 * and the glow accent, so L0 and L1 have a token form and L2/L3 do not. This is
 * also the per-layer failure fallback of §8.3: a failed layer takes its T0 form
 * and every sibling keeps whatever it had resolved.
 */
function t0Treatment(layer: EnvLayerId): EnvLayerTreatment {
  return layer === 'l0' || layer === 'l1' ? 'token-gradient' : 'off';
}

/** Apply the §4.5 portrait recomposition on top of the tier treatment. */
function portraitTreatment(layer: EnvLayerId, base: EnvLayerTreatment): EnvLayerTreatment {
  if (base === 'off') return 'off';
  // L0 is the token gradient extended above and below the plate (no fetch), L2
  // renders only its two lips re-anchored to the canvas, L3 is off, and L1 is
  // cover-fit — a recomposition, not a crop.
  if (layer === 'l0') return 'token-gradient';
  if (layer === 'l2') return 'lips-only';
  if (layer === 'l3') return 'off';
  return base;
}

/** The ambient-motion level (§7.1): off entirely under reduced motion, at any level. */
export function ambientLevel(quality: EffectQuality, reducedMotion: boolean): EnvAmbient {
  if (reducedMotion || quality === 'lite') return 'off';
  return quality === 'high' ? 'l0+l3' : 'l0-half';
}

/** The passive hooks permitted at a tier (§8.1): none at Lite, all but one at Standard. */
export function hooksFor(quality: EffectQuality, reducedMotion: boolean): readonly EnvHook[] {
  if (quality === 'lite' || reducedMotion) return [];
  if (quality === 'standard') return ENV_HOOKS.filter((hook) => hook !== STANDARD_SUPPRESSED);
  return ENV_HOOKS;
}

/**
 * Resolve the environment plan for one frame. Pure, total, and never throwing:
 * there is no input for which this returns an absent or empty environment, which
 * is the §8.3 guarantee ("no state in which the environment is a hole, a flat
 * black field, or a blocker") expressed as a function signature.
 */
export function planEnvironment(input: EnvironmentPlanInput): EnvironmentPlan {
  const { quality, reducedMotion, viewport } = input;
  const portrait = isPortraitViewport(viewport);
  const failed = new Set(input.failedKeys ?? []);

  // Whole-theme failure: every variant the tier would draw has failed. Fall back
  // to the default theme at T0 — the theme still reads, and nothing is modal.
  const requested = ENV_MANIFESTS[input.theme] ?? ENV_MANIFESTS[DEFAULT_SCENE_THEME];
  const anyResolvable = Object.values(requested.assets).some((asset) => !failed.has(asset.key));
  const themeFellBack = !anyResolvable && requested.theme !== DEFAULT_SCENE_THEME;
  const manifest = themeFellBack ? ENV_MANIFESTS[DEFAULT_SCENE_THEME] : requested;

  const excursionBase = baseExcursionPx(viewport);
  const excursionPx = reducedMotion
    ? 0
    : quality === 'lite'
      ? 0
      : quality === 'standard'
        ? excursionBase / 2
        : excursionBase;

  // A `composed` study is one flattened plate in the L1 slot. It only *engages*
  // when that plate is the thing L1 is about to draw: at Lite the tier asks for
  // the half-resolution variant, which no study ships, and a failed plate has
  // to leave the full procedural composition standing (§8.3 — "the theme still
  // reads"), so both cases fall through to the layered placeholder untouched.
  const composedPlate = manifest.assets.l1;
  const composedActive =
    manifest.composition === 'composed' &&
    composedPlate.source === 'raster' &&
    !failed.has(composedPlate.key) &&
    !themeFellBack &&
    variantFor('l1', quality) === 'l1';

  const layers = (Object.keys(ENV_LAYERS) as EnvLayerId[]).map((layer): EnvLayerPlan => {
    const variant = variantFor(layer, quality);
    let treatment = baseTreatment(layer, quality);
    if (portrait) treatment = portraitTreatment(layer, treatment);
    // The study carries surround, edge, and props in one image: L0 drops to its
    // zero-byte token form behind the opaque plate, and L2/L3 stand down rather
    // than drawing a second rim and a second set of props over the first.
    if (composedActive && layer !== 'l1') treatment = layer === 'l0' ? 'token-gradient' : 'off';
    const asset = variant ? manifest.assets[variant] : undefined;
    // A failed key falls back to its T0 token treatment; its siblings are
    // untouched, so one missing plate never cascades.
    const failedHere = asset !== undefined && failed.has(asset.key);
    // Whole-theme fallback renders the default theme at T0, not at its plates.
    const degraded = failedHere || themeFellBack;
    if (degraded) treatment = t0Treatment(layer);
    const draws = treatment === 'plate' || treatment === 'lips-only';
    // Only a full `plate` can be a raster: `lips-only` re-anchors two bands to
    // the canvas (§4.5), which a plate with its lips baked in cannot express, so
    // portrait L2 always takes the procedural form.
    const raster = treatment === 'plate' && asset?.source === 'raster' ? asset.src : undefined;
    return {
      layer,
      treatment,
      variant: draws ? variant : undefined,
      key: draws && asset !== undefined ? asset.key : undefined,
      ...(raster === undefined ? undefined : { rasterPath: raster }),
      ...(raster !== undefined && layer === 'l3' && manifest.atlas
        ? { atlas: manifest.atlas }
        : undefined),
      ...(composedActive && layer === 'l1' ? { composed: true } : undefined),
      parallaxPx: Math.round(ENV_LAYERS[layer].parallax * excursionPx * 100) / 100,
      degraded,
    };
  });

  return {
    theme: manifest.theme,
    manifest,
    themeFellBack,
    layers,
    excursionPx,
    ambient: ambientLevel(quality, reducedMotion),
    portrait,
    hooks: hooksFor(quality, reducedMotion),
    composition: manifest.composition,
    composedActive,
  };
}
