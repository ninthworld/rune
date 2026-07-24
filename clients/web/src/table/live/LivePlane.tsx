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
import type { GameView, PlayerId } from '../../protocol';
import { stagePlane, type PlaneRegion, type PlaneStagingState, type StagedPlane } from '../plane';
import { cardFaceRenderer } from '../planeFaceRenderer';
import { planeDisplayData } from '../planeDisplayData';
import { PlaneReconciler, planeRegions, planeRenders } from '../planeReconciler';
import { EffectsLayer, type EffectDensity, type EffectQuality } from '../effects';
import { EffectsSurface } from '../EffectsSurface';
import { LivePlaneControls, type LivePlaneInteractionProps } from './LivePlaneControls';
import {
  deriveGameViewPresentation,
  freezeDepartedEffectAnchors,
  type GameViewPresentation,
  type TargetingPresentationPath,
} from './gameViewPresentation';
import {
  determinePresentationMode,
  orientationCue,
  rebuildBudgetMs,
  SCENE_DOM_CEILING,
  type PresentationMode,
  type RebuildSample,
} from './presentationMode';
import styles from './live-plane.module.css';

/** Monotonic clock for rebuild timing; degrades to 0 where unavailable. */
function monotonicNow(): number {
  return typeof performance !== 'undefined' ? performance.now() : 0;
}

type SceneStyle = CSSProperties & Record<`--${string}`, string | number>;

const sceneStyle: SceneStyle = {
  '--ink': SCENE_NEUTRALS.ink,
  '--surface-base': SCENE_NEUTRALS.surfaceBase,
  '--raised': SCENE_NEUTRALS.raised,
  '--text': SCENE_NEUTRALS.text,
  '--gold': SCENE_HUES.gold.value,
  '--blue': SCENE_HUES.blue.value,
  '--orange': SCENE_HUES.orange.value,
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
  /**
   * The store's transport generation ({@link GameStore.sessionEpoch}). A view
   * arriving under a higher epoch than the one last presented is a reconnect /
   * resync discontinuity and is rebuilt from scratch rather than reconciled
   * (issue #493). Omitted ⇒ every in-session view reconciles incrementally.
   */
  sessionEpoch?: number;
  /** Server-driven pending target paths assembled by the interaction adapter. */
  targetingPaths?: readonly TargetingPresentationPath[];
  /** Publish the current pure plane to test/read-only consumers. */
  onPlane?: (plane: StagedPlane) => void;
  /** Publish the pure view-delta adapter result to controlled tests/consumers. */
  onPresentation?: (presentation: GameViewPresentation) => void;
  /** Notified with the mode chosen for each presented view (initial/reconcile/rebuild/fast-forward). */
  onMode?: (mode: PresentationMode) => void;
  /** Notified with the measured budget sample after each full rebuild / collapse. */
  onRebuild?: (sample: RebuildSample) => void;
  /** Server-authoritative semantic controls layered over staged destinations. */
  interaction?: LivePlaneInteractionProps;
}

function initialSize(): PlaneSize {
  if (typeof window === 'undefined') return { width: 1280, height: 720 };
  return {
    width: Math.max(320, window.innerWidth),
    height: Math.max(360, window.innerHeight),
  };
}

function refreshVisualAnchors(
  anchors: Map<string, { x: number; y: number; w: number; h: number }>,
  plane: StagedPlane,
  reconciler: PlaneReconciler,
  view: GameView,
): void {
  anchors.clear();
  for (const render of planeRenders(plane)) {
    const rect = reconciler.visualFor(render.entityId);
    if (rect) {
      for (const memberId of render.memberIds) anchors.set(memberId, rect);
    }
  }
  const addRegion = (region: PlaneRegion): void => {
    anchors.set(
      `seat:${region.seat}`,
      reconciler.chromeVisualFor(`crest:${region.seat}`) ?? region.crest,
    );
    anchors.set(
      `pile:${region.seat}`,
      reconciler.chromeVisualFor(`piles:${region.seat}`) ?? region.piles,
    );
  };
  for (const region of planeRegions(plane)) addRegion(region);
  for (const tile of plane.tiles) {
    const visual = reconciler.chromeVisualFor(`tile:${tile.seat}`);
    const dx = visual === undefined ? 0 : visual.x - tile.rect.x;
    const dy = visual === undefined ? 0 : visual.y - tile.rect.y;
    anchors.set(`seat:${tile.seat}`, { ...tile.crest, x: tile.crest.x + dx, y: tile.crest.y + dy });
    anchors.set(`pile:${tile.seat}`, visual ?? tile.rect);
  }
  // Off-focus combat staging (issue #501): a permanent the ladder did not draw
  // individually — a digest-rung wing's board, a compact seat behind its tile —
  // still anchors at its controller's crest, which is staged at every rung. An
  // attack path from an unstaged attacker therefore draws to the defender's
  // crest instead of being retired for an unresolvable endpoint, so combat
  // against or by any seat is visible regardless of which board holds focus.
  for (const permanent of view.battlefield) {
    if (anchors.has(permanent.id)) continue;
    const crest = anchors.get(`seat:${permanent.controller}`);
    if (crest) anchors.set(permanent.id, crest);
  }
  if (plane.receiver) {
    anchors.set(`hand:${view.you}`, {
      x: plane.receiver.rect.x + plane.receiver.rect.w / 2 - 24,
      y: Math.min(plane.height - 8, plane.receiver.rect.y + plane.receiver.rect.h),
      w: 48,
      h: 68,
    });
  }
  for (let index = 0; index < view.stack.length; index += 1) {
    const item = view.stack[index]!;
    anchors.set(`stack:${item.id}`, {
      x: plane.corridor.x + plane.corridor.w / 2 - 24 + index * 3,
      y: plane.corridor.y + plane.corridor.h / 2 - 34 - index * 3,
      w: 48,
      h: 68,
    });
  }
}

/** Render the latest live match view on the Phase 1 scene stack. */
export function LivePlane({
  view,
  staging,
  quality,
  density,
  reducedMotion,
  artVersion,
  sessionEpoch,
  targetingPaths,
  onPlane,
  onPresentation,
  onMode,
  onRebuild,
  interaction,
}: LivePlaneProps) {
  const [size, setSize] = useState<PlaneSize>(initialSize);
  const hostRef = useRef<HTMLDivElement | null>(null);
  const planeRootRef = useRef<HTMLDivElement | null>(null);
  const reconcilerRef = useRef<PlaneReconciler | null>(null);
  const rafRef = useRef(0);
  const anchorsRef = useRef(new Map<string, { x: number; y: number; w: number; h: number }>());
  const viewRef = useRef(view);
  const stagingRef = useRef(staging);
  const targetingPathsRef = useRef(targetingPaths);
  const previousViewRef = useRef<GameView>();
  const previousFocusRef = useRef<PlayerId>();
  // The transport generation last presented, and the current one, read inside the
  // reconcile effect without re-running it on a bare epoch bump (the rebuild is
  // driven by the *view* that arrives after a reconnect, not the bump itself).
  const presentedEpochRef = useRef(sessionEpoch ?? 0);
  const sessionEpochRef = useRef(sessionEpoch ?? 0);
  // Instrumentation callbacks read via refs so their identity never re-runs the
  // reconcile effect (which would spuriously re-present the same view).
  const onModeRef = useRef(onMode);
  const onRebuildRef = useRef(onRebuild);
  viewRef.current = view;
  stagingRef.current = staging;
  targetingPathsRef.current = targetingPaths;
  sessionEpochRef.current = sessionEpoch ?? 0;
  onModeRef.current = onMode;
  onRebuildRef.current = onRebuild;

  const plane = useMemo(() => stagePlane(view, size, staging), [size, staging, view]);
  const planeRef = useRef(plane);
  planeRef.current = plane;

  // Environmental animation steps on → reduced → off across quality levels
  // (presentation-budgets §Quality levels); reduced motion turns it off at any
  // level. Purely the ambient backdrop — the scene is never touched.
  const environmentMotion =
    reducedMotion || quality === 'lite' ? 'off' : quality === 'high' ? 'on' : 'reduced';

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
      refreshVisualAnchors(anchorsRef.current, planeRef.current, reconciler, viewRef.current);
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

  const emitRebuildSample = useCallback(
    (mode: PresentationMode, start: number, compact: boolean, root: HTMLElement): void => {
      const report = onRebuildRef.current;
      if (!report) return;
      const durationMs = monotonicNow() - start;
      // Count the reconciled scene subtree — the DOM that scales with the board,
      // which the ≤15k budget governs (chrome is fixed and lives outside it).
      const domNodes = root.querySelectorAll('*').length;
      const budgetMs = rebuildBudgetMs(compact);
      report({
        mode,
        durationMs,
        domNodes,
        budgetMs,
        withinBudget: durationMs <= budgetMs,
        withinDomCeiling: domNodes <= SCENE_DOM_CEILING,
      });
    },
    [],
  );

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
    const start = monotonicNow();
    reconciler.rebuild(planeRef.current);
    anchorsRef.current.clear();
    refreshVisualAnchors(anchorsRef.current, planeRef.current, reconciler, viewRef.current);
    // A fresh reconciler starts with no live effects; there is nothing yet to
    // clear, and the initial mount carries no orientation cue (game start owns
    // its own treatment) — only persistent paths for a mid-game first frame.
    effectsLayer.setPersistent(
      deriveGameViewPresentation(undefined, viewRef.current, {
        focusSeat: planeRef.current.focusSeat,
        targetingPaths: targetingPathsRef.current,
        quality,
        reducedMotion,
      }).persistent,
    );
    presentedEpochRef.current = sessionEpochRef.current;
    emitRebuildSample('initial', start, planeRef.current.compact, root);
    onModeRef.current?.('initial');
    return () => {
      if (rafRef.current !== 0) window.cancelAnimationFrame(rafRef.current);
      rafRef.current = 0;
      reconciler.clear();
      root.replaceChildren();
      if (reconcilerRef.current === reconciler) reconcilerRef.current = null;
    };
  }, [effectsLayer, emitRebuildSample, quality, reducedMotion]);

  useLayoutEffect(() => {
    const reconciler = reconcilerRef.current;
    if (!reconciler) return;
    const root = planeRootRef.current;
    const hasPreviousView = previousViewRef.current !== undefined;
    const superseding = hasPreviousView && previousViewRef.current !== view;
    const mode = determinePresentationMode({
      hasPreviousView,
      discontinuity: hasPreviousView && sessionEpochRef.current !== presentedEpochRef.current,
      presentationBusy: superseding && reconciler.hasPendingAnimations(),
    });
    // Persistent paths/links are a pure function of the current view — always
    // rebuilt from it alone so combat/targeting overlays are correct after any
    // discontinuity, never carried across a rebuild.
    const currentPersistent = (): GameViewPresentation['persistent'] =>
      deriveGameViewPresentation(undefined, view, {
        focusSeat: plane.focusSeat,
        targetingPaths,
        quality,
        reducedMotion,
      }).persistent;

    if (mode === 'rebuild') {
      // Reconnect / resync: rebuild the latest complete view with motion
      // suppressed, clearing every ghost, transient, path, and cached anchor
      // before the rebuilt scene is exposed. Ephemeral selection/prompt UI is
      // cleared in parallel by LiveMatchTable's per-view reset.
      const start = monotonicNow();
      reconciler.discardMotionProxies();
      reconciler.rebuild(plane);
      anchorsRef.current.clear();
      refreshVisualAnchors(anchorsRef.current, plane, reconciler, view);
      // The single non-blocking "you are here" cue on the active crest; reduced
      // motion receives the complete final state with no pulse.
      effectsLayer.replaceTransients(orientationCue(view, reducedMotion));
      effectsLayer.setPersistent(currentPersistent());
      if (root) emitRebuildSample('rebuild', start, plane.compact, root);
    } else if (mode === 'fast-forward') {
      // A newer view outran the prior transition: collapse to it. Snap the
      // in-flight motion onto its final layout, drop obsolete proxies and
      // transients, and reconcile the newest plane without catch-up travel —
      // gameplay is never queued behind animation.
      const start = monotonicNow();
      reconciler.skipTransitions();
      reconciler.discardMotionProxies();
      reconciler.reconcile(plane, [], true);
      refreshVisualAnchors(anchorsRef.current, plane, reconciler, view);
      effectsLayer.replaceTransients([]);
      effectsLayer.setPersistent(currentPersistent());
      if (root) emitRebuildSample('fast-forward', start, plane.compact, root);
    } else {
      // initial | reconcile — the ordinary incremental animated path.
      const previousAnchors = new Map(anchorsRef.current);
      const presentation = deriveGameViewPresentation(previousViewRef.current, view, {
        previousFocusSeat: previousFocusRef.current,
        // The focus the plane RESOLVED (manual or default relevance), so the
        // staging cue and the off-focus channel agree with what is staged.
        focusSeat: plane.focusSeat,
        targetingPaths,
        quality,
        reducedMotion,
      });
      if (superseding) reconciler.discardMotionProxies();
      reconciler.reconcile(plane, presentation.motions);
      refreshVisualAnchors(anchorsRef.current, plane, reconciler, view);
      effectsLayer.setPersistent(presentation.persistent);
      if (superseding) {
        effectsLayer.replaceTransients(
          freezeDepartedEffectAnchors(presentation.transients, previousAnchors, anchorsRef.current),
        );
      }
      startMotion();
      onPresentation?.(presentation);
    }

    onModeRef.current?.(mode);
    onPlane?.(plane);
    previousViewRef.current = view;
    previousFocusRef.current = plane.focusSeat;
    presentedEpochRef.current = sessionEpochRef.current;
  }, [
    artVersion,
    effectsLayer,
    emitRebuildSample,
    onPlane,
    onPresentation,
    plane,
    quality,
    reducedMotion,
    startMotion,
    targetingPaths,
    view,
  ]);

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
      <div
        className={styles.environment}
        data-environment={environmentMotion}
        data-testid="live-environment"
        aria-hidden="true"
      >
        <div className={styles.sky} />
        <div className={styles.ground} />
        <div className={styles.arenaEdge} />
        <div className={styles.tableMark}>◇</div>
      </div>
      <div className={styles.camera}>
        <div className={styles.tiltedPlane}>
          <div ref={planeRootRef} className={styles.plane} data-testid="live-plane-dom" />
          {interaction && <LivePlaneControls view={view} plane={plane} interaction={interaction} />}
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
