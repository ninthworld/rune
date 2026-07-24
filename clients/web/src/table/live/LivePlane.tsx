/**
 * The live production mount for ADR 0030 layers 1–3:
 * environment, DOM scene plane, and the passive WebGL effects overlay.
 *
 * It consumes only the latest `GameView` plus ephemeral staging preferences.
 * First mount is a motion-suppressed full rebuild; every later view is an
 * incremental reconcile by entity id. The animation clock runs on demand and
 * stops as soon as the reconciler has no pending work.
 */
import { useCallback, useLayoutEffect, useMemo, useRef, useState, type CSSProperties } from 'react';
import {
  DEFAULT_SCENE_THEME,
  SCENE_ELEVATION,
  SCENE_HUES,
  SCENE_NEUTRALS,
  SCENE_SEAT_ACCENTS,
  SCENE_THEMES,
} from '../../sceneTokens';
import type { GameView } from '../../protocol';
import { stagePlane, type PlaneRegion, type PlaneStagingState, type StagedPlane } from '../plane';
import { cardFaceRenderer } from '../planeFaceRenderer';
import { planeDisplayData } from '../planeDisplayData';
import { PlaneReconciler, planeRegions, planeRenders } from '../planeReconciler';
import { EffectsLayer, type EffectDensity, type EffectQuality } from '../effects';
import { EffectsSurface } from '../EffectsSurface';
import styles from './live-plane.module.css';

type SceneStyle = CSSProperties & Record<`--${string}`, string | number>;

const sceneStyle: SceneStyle = {
  '--ink': SCENE_NEUTRALS.ink,
  '--surface-base': SCENE_NEUTRALS.surfaceBase,
  '--raised': SCENE_NEUTRALS.raised,
  '--text': SCENE_NEUTRALS.text,
  '--gold': SCENE_HUES.gold.value,
  '--blue': SCENE_HUES.blue.value,
  '--seat-azure': SCENE_SEAT_ACCENTS[0],
  '--seat-amethyst': SCENE_SEAT_ACCENTS[3],
  '--seat-teal': SCENE_SEAT_ACCENTS[5],
  '--shadow-rest': SCENE_ELEVATION.rest.shadow,
  '--sky-top': SCENE_THEMES[DEFAULT_SCENE_THEME].skyTop,
  '--sky-horizon': SCENE_THEMES[DEFAULT_SCENE_THEME].skyHorizon,
  '--sky-base': SCENE_THEMES[DEFAULT_SCENE_THEME].skyBase,
  '--far-ground': SCENE_THEMES[DEFAULT_SCENE_THEME].ground,
  '--arena': SCENE_THEMES[DEFAULT_SCENE_THEME].arena,
  '--ambient-glow': SCENE_THEMES[DEFAULT_SCENE_THEME].glow,
};

interface PlaneSize {
  width: number;
  height: number;
}

/** Inputs for the production scene-plane mount. */
export interface LivePlaneProps {
  /** Latest complete personalized view. */
  view: GameView;
  /** Ephemeral focus/selection/candidate staging; never authoritative. */
  staging?: PlaneStagingState;
  /** Effect quality budget. */
  quality: EffectQuality;
  /** Effect-density preference, independent of quality. */
  density: EffectDensity;
  /** Whether all motion collapses to its final state. */
  reducedMotion: boolean;
  /** Art-store version; a change rechecks DOM face signatures. */
  artVersion?: number;
  /** Publish the current pure plane to test/read-only consumers. */
  onPlane?: (plane: StagedPlane) => void;
}

function initialSize(): PlaneSize {
  if (typeof window === 'undefined') return { width: 1280, height: 720 };
  return {
    width: Math.max(320, window.innerWidth),
    height: Math.max(360, window.innerHeight),
  };
}

function anchorPlane(
  anchors: Map<string, { x: number; y: number; w: number; h: number }>,
  plane: StagedPlane,
  reconciler: PlaneReconciler,
): void {
  anchors.clear();
  for (const render of planeRenders(plane)) {
    const rect = reconciler.targetFor(render.entityId);
    if (rect) anchors.set(render.entityId, rect);
  }
  const addRegion = (region: PlaneRegion): void => {
    anchors.set(`seat:${region.seat}`, region.crest);
  };
  for (const region of planeRegions(plane)) addRegion(region);
  for (const tile of plane.tiles) anchors.set(`seat:${tile.seat}`, tile.crest);
}

/** Render the latest live match view on the Phase 1 scene stack. */
export function LivePlane({
  view,
  staging,
  quality,
  density,
  reducedMotion,
  artVersion,
  onPlane,
}: LivePlaneProps) {
  const [size, setSize] = useState<PlaneSize>(initialSize);
  const hostRef = useRef<HTMLDivElement | null>(null);
  const planeRootRef = useRef<HTMLDivElement | null>(null);
  const reconcilerRef = useRef<PlaneReconciler | null>(null);
  const rafRef = useRef(0);
  const anchorsRef = useRef(new Map<string, { x: number; y: number; w: number; h: number }>());
  const viewRef = useRef(view);
  const stagingRef = useRef(staging);
  viewRef.current = view;
  stagingRef.current = staging;

  const plane = useMemo(() => stagePlane(view, size, staging), [size, staging, view]);
  const planeRef = useRef(plane);
  planeRef.current = plane;

  const effectsLayer = useMemo(
    () =>
      new EffectsLayer({
        quality,
        density,
        reducedMotion,
        rects: (ref) => anchorsRef.current.get(ref),
      }),
    [density, quality, reducedMotion],
  );

  const startMotion = useCallback(() => {
    if (rafRef.current !== 0) return;
    const tick = (now: number): void => {
      const reconciler = reconcilerRef.current;
      if (!reconciler) {
        rafRef.current = 0;
        return;
      }
      reconciler.advance(now);
      const moving = reconciler.hasPendingAnimations();
      effectsLayer.trackMotion(moving);
      if (moving) {
        rafRef.current = window.requestAnimationFrame(tick);
      } else {
        rafRef.current = 0;
      }
    };
    if (reconcilerRef.current?.hasPendingAnimations()) {
      rafRef.current = window.requestAnimationFrame(tick);
    }
  }, [effectsLayer]);

  useLayoutEffect(() => {
    const root = planeRootRef.current;
    if (!root) return;
    root.replaceChildren();
    const reconciler = new PlaneReconciler(root, {
      face: cardFaceRenderer((render) =>
        planeDisplayData(viewRef.current, stagingRef.current, render),
      ),
      animate: { reducedMotion },
    });
    reconcilerRef.current = reconciler;
    // First frame is a reconnect-safe complete scene: no enter/travel motion.
    reconciler.rebuild(planeRef.current);
    anchorPlane(anchorsRef.current, planeRef.current, reconciler);
    effectsLayer.setPersistent([]);
    return () => {
      if (rafRef.current !== 0) window.cancelAnimationFrame(rafRef.current);
      rafRef.current = 0;
      reconciler.clear();
      root.replaceChildren();
      if (reconcilerRef.current === reconciler) reconcilerRef.current = null;
    };
  }, [effectsLayer, reducedMotion]);

  useLayoutEffect(() => {
    const reconciler = reconcilerRef.current;
    if (!reconciler) return;
    reconciler.reconcile(plane);
    anchorPlane(anchorsRef.current, plane, reconciler);
    startMotion();
    onPlane?.(plane);
  }, [artVersion, onPlane, plane, startMotion]);

  useLayoutEffect(() => {
    const host = hostRef.current;
    if (!host) return;
    const resize = (): void => {
      const rect = host.getBoundingClientRect();
      if (rect.width <= 0 || rect.height <= 0) return;
      const next = {
        width: Math.max(320, Math.round(rect.width)),
        height: Math.max(360, Math.round(rect.height)),
      };
      setSize((current) =>
        current.width === next.width && current.height === next.height ? current : next,
      );
    };
    resize();
    const observer = typeof ResizeObserver === 'undefined' ? null : new ResizeObserver(resize);
    observer?.observe(host);
    window.addEventListener('resize', resize);
    return () => {
      observer?.disconnect();
      window.removeEventListener('resize', resize);
    };
  }, []);

  return (
    <div
      ref={hostRef}
      className={styles.host}
      data-testid="live-2-5d-plane"
      data-compact={String(plane.compact)}
      style={sceneStyle}
      aria-label="2.5D battlefield"
    >
      <div className={styles.environment} aria-hidden="true">
        <div className={styles.sky} />
        <div className={styles.ground} />
        <div className={styles.arenaEdge} />
        <div className={styles.tableMark}>◇</div>
      </div>
      <div className={styles.camera}>
        <div className={styles.tiltedPlane}>
          <div ref={planeRootRef} className={styles.plane} data-testid="live-plane-dom" />
          <div className={styles.effects}>
            <EffectsSurface
              key={`${quality}:${density}:${String(reducedMotion)}`}
              layer={effectsLayer}
              width={plane.width}
              height={plane.height}
            />
          </div>
        </div>
      </div>
    </div>
  );
}
