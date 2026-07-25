/**
 * The one battlefield environment mount (`docs/design/environment-system.md`) —
 * issue #530. Shared by the match (`table/live/LivePlane.tsx`) and the pregame
 * places (`pregame/PregameStage.tsx`), so there is exactly one environment
 * system rather than two that drift.
 *
 * ADR 0030 layer 1. Four nodes, z-ordered `L0 → L1 → L2 → L3`, strictly behind
 * the scene plane. What this component guarantees, at every quality level, in
 * every theme, on every device (§7):
 *
 * 1. Every layer carries `pointer-events: none` and `aria-hidden`. No layer is
 *    ever a hit target, a drop target, a focus stop, or a tab stop.
 * 2. Nothing here appears in `valid_actions[]`, in a prompt's candidate list, or
 *    in any hit-test result. The client computes no legality.
 * 3. The environment carries **no state that survives a view**: it is a pure
 *    function of `(theme, viewport, quality, reduced motion, failed keys)`, so a
 *    reconnect renders it identically with animation suppressed.
 * 4. It never occludes a game object and never overlaps a card, crest, pile, or
 *    path.
 * 5. It never gates input, never delays a scene build, and never blocks a match
 *    on an asset — the match is fully interactive at T0, before any layer
 *    resolves.
 *
 * **Loading (§8.2).** T0 is the token composition and is always the first frame,
 * at zero bytes. T1 is the layered SVG placeholder of §10 — also zero bytes and
 * zero requests, because the placeholder is code. T2 is the raster plates of
 * issue #548, which landed in #555: a slot whose manifest entry resolved to
 * `source: 'raster'` takes the `<img>` branch below, and every other slot keeps
 * the placeholder. The match is interactive at T0 either way, and a plate that
 * never arrives changes nothing but the pixels.
 *
 * **Not implemented here:** the five passive reaction hooks of §7.2. The tier
 * gate that decides which are permitted is resolved (`plan.hooks`) and tested,
 * but nothing subscribes them to the presentation intent stream yet — that
 * touches `live/gameViewPresentation.ts`, which is outside this issue's layer.
 */
import { useEffect, useMemo, useRef, useState } from 'react';
import type { EffectQuality } from '../effects';
import { usePresentationSettings } from '../settings/usePresentationSettings';
import { EnvLayerL0, EnvLayerL1, EnvLayerL2, EnvLayerL3 } from './EnvironmentLayers';
import { environmentSceneVars } from './environmentScene';
import { cropForViewport } from './crop';
import { planEnvironment, type EnvBias, type EnvLayerPlan, type EnvViewport } from './quality';
import {
  propFootprint,
  type EnvManifestKey,
  type EnvPropAtlas,
  type EnvPropEntry,
} from './manifest';
import s from './environment.module.css';

/** Inputs for the shared environment mount. */
export interface SceneEnvironmentProps {
  /** The device-local quality level (`presentation-budgets.md` §Quality levels). */
  quality: EffectQuality;
  /** The composed `prefers-reduced-motion` result. */
  reducedMotion: boolean;
  /**
   * The parallax bias for this frame, driven **only** by the staging tween's
   * plane delta (§1.1) — never by pointer position, device orientation, or
   * scroll, because ADR 0030 has no free camera. Omitted ⇒ centred.
   */
  bias?: EnvBias;
  /**
   * The viewport the environment composes into. Omitted ⇒ measured from the
   * window, which is what both call sites want (the environment is full-bleed).
   */
  viewport?: EnvViewport;
  /**
   * Manifest keys treated as failed, for the §8.3 fallback. Production supplies
   * these from the raster branch's `onError`; tests may also supply them
   * directly, to reach a fallback without waiting on a decode jsdom never does.
   */
  failedKeys?: readonly EnvManifestKey[];
}

/** The window's logical size, SSR-safe. */
function readViewport(): EnvViewport {
  if (typeof window === 'undefined') return { width: 1280, height: 720 };
  return { width: Math.max(1, window.innerWidth), height: Math.max(1, window.innerHeight) };
}

/** Track the window size for the aspect crop and the excursion ladder. */
function useWindowViewport(override: EnvViewport | undefined): EnvViewport {
  const [measured, setMeasured] = useState<EnvViewport>(readViewport);
  useEffect(() => {
    if (override !== undefined || typeof window === 'undefined') return;
    const onResize = (): void => {
      const next = readViewport();
      setMeasured((current) =>
        current.width === next.width && current.height === next.height ? current : next,
      );
    };
    onResize();
    window.addEventListener('resize', onResize);
    return () => window.removeEventListener('resize', onResize);
  }, [override]);
  return override ?? measured;
}

/** A stable id prefix so two mounted environments never share a gradient id. */
let mountCounter = 0;

export function SceneEnvironment({
  quality,
  reducedMotion,
  bias,
  viewport,
  failedKeys,
}: SceneEnvironmentProps) {
  const settings = usePresentationSettings();
  const size = useWindowViewport(viewport);
  const idPrefix = useRef<string>();
  idPrefix.current ??= `env${(mountCounter += 1)}`;

  // Failures observed by the raster branch (§8.3). A key that lands here falls
  // its own layer back to the T0 token treatment and touches no sibling.
  const [runtimeFailed, setRuntimeFailed] = useState<readonly EnvManifestKey[]>([]);
  const failed = useMemo(
    () => [...(failedKeys ?? []), ...runtimeFailed],
    [failedKeys, runtimeFailed],
  );

  const plan = useMemo(
    () =>
      planEnvironment({
        theme: settings.environmentTheme,
        quality,
        reducedMotion,
        viewport: size,
        failedKeys: failed,
      }),
    [failed, quality, reducedMotion, settings.environmentTheme, size],
  );

  const crop = useMemo(() => cropForViewport(size), [size]);
  const vars = useMemo(() => environmentSceneVars(plan, reducedMotion), [plan, reducedMotion]);
  const bx = bias?.x ?? 0;
  const by = bias?.y ?? 0;

  return (
    <div
      className={s.environment}
      style={vars}
      data-testid="scene-environment"
      data-theme={plan.theme}
      data-ambient={plan.ambient}
      data-portrait={String(plan.portrait)}
      data-theme-fallback={String(plan.themeFellBack)}
      data-composition={plan.composition}
      data-composed={String(plan.composedActive)}
      aria-hidden="true"
    >
      {plan.layers.map((layer) => (
        <EnvLayer
          key={layer.layer}
          plan={layer}
          crop={crop.viewBox}
          idPrefix={idPrefix.current!}
          props={plan.manifest.props}
          viewport={size}
          bias={{ x: bx, y: by }}
          onFailed={(key) =>
            setRuntimeFailed((current) => (current.includes(key) ? current : [...current, key]))
          }
        />
      ))}
    </div>
  );
}

/** One layer node: the same slot whether it draws SVG, a plate, or its T0 form. */
function EnvLayer({
  plan,
  crop,
  idPrefix,
  props,
  viewport,
  bias,
  onFailed,
}: {
  plan: EnvLayerPlan;
  crop: string;
  idPrefix: string;
  props: readonly EnvPropEntry[];
  viewport: EnvViewport;
  bias: EnvBias;
  onFailed: (key: EnvManifestKey) => void;
}) {
  if (plan.treatment === 'off') return null;
  // Parallax: `factor × E`, already resolved to px by the plan, displaced by the
  // staging tween's bias. Transform only — no layout, no reflow, and far below
  // the 44 px hit floor, so nothing a player aims at can move.
  const style = {
    transform: `translate3d(${plan.parallaxPx * bias.x}px, ${plan.parallaxPx * bias.y}px, 0)`,
  };
  return (
    <div
      className={s.layer}
      style={style}
      data-layer={plan.layer}
      data-treatment={plan.treatment}
      data-degraded={String(plan.degraded)}
      data-key={plan.key}
      data-source={plan.rasterPath === undefined ? 'procedural' : 'raster'}
      aria-hidden="true"
    >
      <LayerArt
        plan={plan}
        crop={crop}
        idPrefix={idPrefix}
        props={props}
        viewport={viewport}
        onFailed={onFailed}
      />
    </div>
  );
}

/** The art inside one layer node — a shipped plate, or the procedural placeholder. */
function LayerArt({
  plan,
  crop,
  idPrefix,
  props,
  viewport,
  onFailed,
}: {
  plan: EnvLayerPlan;
  crop: string;
  idPrefix: string;
  props: readonly EnvPropEntry[];
  viewport: EnvViewport;
  onFailed: (key: EnvManifestKey) => void;
}) {
  // T0 / per-layer failure / Lite L0: the token composition. L0's is the surround
  // gradient and L1's is the plaza ellipse with its medallion — both rendered by
  // the same components, which is why the placeholder is permanent rather than a
  // stopgap (§10.5, last paragraph).
  if (plan.treatment === 'token-gradient') {
    return plan.layer === 'l0' ? (
      <div className={s.tokenSurround} />
    ) : (
      <EnvLayerL1 viewBox={crop} idPrefix={idPrefix} />
    );
  }

  // The raster branch (§10.5 step 3). A plate that fails to load falls this
  // layer back to its T0 form without touching any sibling (§8.3), which is why
  // `onError` reports the key rather than swapping a src.
  if (plan.rasterPath !== undefined && plan.key !== undefined) {
    const key = plan.key;
    // L3 is not a plate (§4.4): the shipped L3 is a sprite atlas, cropped per
    // prop at the anchors the manifest validated against Zone C. Stretching it
    // across the canvas like L0–L2 would both distort it and put props in the
    // focal core.
    if (plan.layer === 'l3' && plan.atlas !== undefined) {
      return (
        <EnvPropSprites
          atlas={plan.atlas}
          props={props}
          viewport={viewport}
          onError={() => onFailed(key)}
        />
      );
    }
    // L0–L2 are one continuous 21:9 plate. `cover` **is** the §4.2 crop: every
    // landscape aspect below 21:9 matches the plate's height and takes a centred
    // horizontal slice, so the medallion authored at (50 %, 40 %) of the source
    // lands at (50 %, 40 %) of the viewport at every aspect — including the §4.5
    // portrait recomposition, which needs no separate anchor for the same
    // reason. Ultrawide reveals rather than stretches.
    return (
      <img
        className={s.plate}
        src={plan.rasterPath}
        alt=""
        aria-hidden="true"
        decoding="async"
        onError={() => onFailed(key)}
      />
    );
  }

  switch (plan.layer) {
    case 'l0':
      return <EnvLayerL0 viewBox={crop} idPrefix={idPrefix} />;
    case 'l1':
      return <EnvLayerL1 viewBox={crop} idPrefix={idPrefix} />;
    case 'l2':
      return (
        <EnvLayerL2 viewBox={crop} idPrefix={idPrefix} lipsOnly={plan.treatment === 'lips-only'} />
      );
    case 'l3':
      return <EnvLayerL3 props={props} />;
  }
}

/**
 * **L3 as raster sprites (§4.4).** Each prop is one atlas frame cropped by an
 * `overflow: hidden` window sitting exactly on the prop's manifest rect.
 *
 * Three properties this preserves, all of them load-bearing:
 *
 * - **Placement is the manifest's, not the atlas's.** The rect comes from
 *   `propRect(anchor, offset, size)` — the same call the procedural silhouettes
 *   use and the one `zones.test.ts` validates against Zone B/C — so the swap
 *   moved pixels and not geometry, and no prop can drift into the focal core.
 * - **Aspect is preserved, and the prop stands on the ground.** The frame is
 *   fitted *inside* the rect (`contain`) and anchored to its bottom centre, so
 *   the drawn sprite is always a subset of the validated footprint.
 * - **Failure is observable.** Each sprite is a real `<img>`, so a missing atlas
 *   reports through `onError` and L3 falls back exactly as §8.3 says. A CSS
 *   `background-image` could not, which is why this is not one.
 *
 * The six sprites share one `src`, so the browser issues one request.
 */
function EnvPropSprites({
  atlas,
  props,
  viewport,
  onError,
}: {
  atlas: EnvPropAtlas;
  props: readonly EnvPropEntry[];
  viewport: EnvViewport;
  onError: () => void;
}) {
  return (
    <>
      {props.map((entry) => {
        const frame = entry.frame;
        if (frame === undefined) return null;
        const rect = propFootprint(entry);
        const boxWidth = rect.w * viewport.width;
        const boxHeight = rect.h * viewport.height;
        // `contain`: the largest scale at which the frame fits the footprint.
        const scale = Math.min(boxWidth / frame.w, boxHeight / frame.h);
        const drawnWidth = frame.w * scale;
        const drawnHeight = frame.h * scale;
        return (
          <div
            key={entry.key}
            className={s.sprite}
            data-prop={entry.key}
            data-anchor={entry.anchor}
            data-mass={entry.mass}
            data-frame={frame.key}
            style={{
              left: `${rect.x * viewport.width + (boxWidth - drawnWidth) / 2}px`,
              top: `${rect.y * viewport.height + (boxHeight - drawnHeight)}px`,
              width: `${drawnWidth}px`,
              height: `${drawnHeight}px`,
            }}
          >
            <img
              className={s.spriteFrame}
              src={atlas.src}
              alt=""
              aria-hidden="true"
              decoding="async"
              onError={onError}
              style={{
                left: `${-frame.x * scale}px`,
                top: `${-frame.y * scale}px`,
                width: `${atlas.width * scale}px`,
                height: `${atlas.height * scale}px`,
              }}
            />
          </div>
        );
      })}
    </>
  );
}
