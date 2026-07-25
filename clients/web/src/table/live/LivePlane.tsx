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
  SCENE_ELEVATION,
  SCENE_FOCUS_DIM,
  SCENE_FRAME_ACCENTS,
  SCENE_HUES,
  SCENE_NEUTRALS,
  SCENE_SEAT_ACCENTS,
} from '../../sceneTokens';
import type { GameView, PlayerId } from '../../protocol';
import {
  RACK_ZONES,
  stagePlane,
  type PlaneRegion,
  type PlaneStagingState,
  type StagedPlane,
} from '../plane';
import { noteCardBackFailed, useCardBack } from '../../card/back';
import { cardFaceRenderer } from '../planeFaceRenderer';
import { planeDisplayData } from '../planeDisplayData';
import { PlaneReconciler, planeRegions, planeRenders } from '../planeReconciler';
import { EffectsLayer, type EffectDensity, type EffectQuality } from '../effects';
import { SceneEnvironment, environmentBias } from '../environment';
import { EffectsSurface } from '../EffectsSurface';
import { presentAudio } from '../audio';
import { LivePlaneControls, type LivePlaneInteractionProps } from './LivePlaneControls';
import {
  deriveGameViewPresentation,
  freezeDepartedEffectAnchors,
  freezeDepartedRelationshipAnchors,
  type GameViewPresentation,
  type TargetingPresentationPath,
} from './gameViewPresentation';
import { attachOccludedEndpoints } from './endpointOcclusion';
import {
  determinePresentationMode,
  orientationCue,
  rebuildBudgetMs,
  SCENE_DOM_CEILING,
  type PresentationMode,
  type RebuildSample,
} from './presentationMode';
import styles from './live-plane.module.css';
// The seat-identity cluster's rules, split out of `live-plane.module.css` at
// the ~800-line ceiling. Both modules declare `.plane`; both class names go on
// the one plane root, so the split changes no selector's scope.
import clusterStyles from './live-plane-cluster.module.css';

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
  '--text-muted': SCENE_NEUTRALS.textMuted,

  // Region grounding (issue #531): a seat's board is not a panel. What is left
  // after the ADR 0032 bands come out is contact shading under the cards — one
  // implied key light, no edge, nothing that reads as chrome.
  '--ground-core': `color-mix(in srgb, ${SCENE_NEUTRALS.ink} 34%, transparent)`,
  '--ground-focus': `color-mix(in srgb, ${SCENE_NEUTRALS.ink} 46%, transparent)`,
  '--ground-edge': `color-mix(in srgb, ${SCENE_NEUTRALS.ink} 12%, transparent)`,

  // Zone-rack materials (zone-geography §3). Each pile's material is derived
  // from a scene token here rather than written as a literal in the stylesheet:
  // the card back's navy field, the graveyard's ash body, and the exile's
  // translucent cyan glass with its bright hairline (§3.3's `#7FB2E5` family is
  // the scene's blue hue, so the pane and the selection ring share one source).
  // The library additionally layers the shipped card-back plate over this field
  // through `--card-back-image`; the token treatment stays underneath, so a
  // missing or failed plate is a colour difference and never a layout one.
  '--rack-back': SCENE_NEUTRALS.surfaceTop,
  '--rack-ash': SCENE_NEUTRALS.raised,
  '--rack-rule': SCENE_HUES.gold.value,
  '--rack-rule-faint': SCENE_NEUTRALS.lineStrong,
  '--rack-glass': `color-mix(in srgb, ${SCENE_HUES.blue.value} 16%, transparent)`,
  '--rack-glass-edge': SCENE_HUES.blue.value,

  // The eliminated treatment, shared with the focus dim (visual-system §3).
  '--dim-brightness': SCENE_FOCUS_DIM.brightness,
  '--dim-saturate': SCENE_FOCUS_DIM.saturate,

  // Seat-identity materials (issue #532, `seat-identity.md` §1.2, §5, §6). The
  // per-seat values — the accent, the portrait plate's URL, and the rung's
  // scale unit `D` — ride on each cluster element as custom properties written
  // by the reconciler; what lives here is the palette every cluster shares.
  //
  // `--crest-stone` is the eliminated rim: the cluster's gold is *replaced*
  // rather than tinted, so the seat reads as out of the game by material and
  // not only by luminance. The frame accents are the identity gem's colour
  // identity — a channel distinct from the seat accent worn by the rim and the
  // turn pennant (§13 conflict 2 records that the two need one owner).
  '--red': SCENE_HUES.red.value,
  '--crest-stone': SCENE_FRAME_ACCENTS.C,
  '--frame-w': SCENE_FRAME_ACCENTS.W,
  '--frame-u': SCENE_FRAME_ACCENTS.U,
  '--frame-b': SCENE_FRAME_ACCENTS.B,
  '--frame-r': SCENE_FRAME_ACCENTS.R,
  '--frame-g': SCENE_FRAME_ACCENTS.G,
  '--frame-m': SCENE_FRAME_ACCENTS.M,
  '--frame-c': SCENE_FRAME_ACCENTS.C,
  '--frame-l': SCENE_FRAME_ACCENTS.L,
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
  // The §7 anchor keys of `docs/design/zone-geography.md`: one `zone:<seat>:<z>`
  // per drawn pile, their union as `zone:<seat>:rack`, and `pile:<seat>` retained
  // as an alias of the union so every shipped motion reference keeps resolving.
  // A digest rack draws one button, and every zone key resolves to it — which is
  // what makes "a motion is retargeted, never retired" hold at every rung.
  const addRegion = (region: PlaneRegion): void => {
    anchors.set(
      `seat:${region.seat}`,
      reconciler.chromeVisualFor(`crest:${region.seat}`) ?? region.crest,
    );
    const digest = region.rack.variant === 'digest';
    for (const slot of region.rack.slots) {
      const key = digest ? `rack:${region.seat}` : `zone:${region.seat}:${slot.zone}`;
      anchors.set(
        `zone:${region.seat}:${slot.zone}`,
        reconciler.chromeVisualFor(key) ?? slot.hitRect,
      );
    }
    const rack = digest
      ? (reconciler.chromeVisualFor(`rack:${region.seat}`) ?? region.piles)
      : region.piles;
    anchors.set(`zone:${region.seat}:rack`, rack);
    anchors.set(`pile:${region.seat}`, rack);
    // `hand:<seat>` for an opponent resolves to their fan's landing slot since
    // #533 (`zone-geography.md` §9: a draw terminates on a real fan slot, not
    // on the crest fallback). The visual rect is preferred so a motion that
    // starts mid-travel leaves from where the back actually is.
    const fan = region.handFan;
    if (fan) {
      const drawn = fan.slots[fan.slots.length - 1];
      anchors.set(
        `hand:${region.seat}`,
        (drawn && reconciler.chromeVisualFor(`handfan:${region.seat}:${drawn.index}`)) ??
          fan.anchor,
      );
    }
  };
  for (const region of planeRegions(plane)) addRegion(region);
  for (const tile of plane.tiles) {
    const visual = reconciler.chromeVisualFor(`tile:${tile.seat}`);
    const dx = visual === undefined ? 0 : visual.x - tile.rect.x;
    const dy = visual === undefined ? 0 : visual.y - tile.rect.y;
    anchors.set(`seat:${tile.seat}`, { ...tile.crest, x: tile.crest.x + dx, y: tile.crest.y + dy });
    const rack = visual ?? tile.rect;
    // A rung-5 summary tile *is* the digest rack (zone-geography §4.1), so every
    // zone key on that seat terminates at the tile rather than nowhere.
    for (const zone of RACK_ZONES) anchors.set(`zone:${tile.seat}:${zone}`, rack);
    anchors.set(`zone:${tile.seat}:rack`, rack);
    anchors.set(`pile:${tile.seat}`, rack);
    // Rung 5 draws no fan (the tile IS the minimal cluster rung), so the hand
    // anchor terminates on the tile — still a real, drawn surface.
    anchors.set(`hand:${tile.seat}`, rack);
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
  // The device's card back (`card-representation.md` §13). A presentation
  // preference, never game state: it takes no card and no view, so it cannot
  // leak what a hidden pile holds, and with nothing resolved every pile keeps
  // its procedural back and the whole plane still rebuilds from this `GameView`.
  const cardBack = useCardBack();
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
      attachOccludedEndpoints(
        deriveGameViewPresentation(undefined, viewRef.current, {
          focusSeat: planeRef.current.focusSeat,
          ...(stagingRef.current?.selectedId === undefined
            ? {}
            : { isolatedId: stagingRef.current.selectedId }),
          targetingPaths: targetingPathsRef.current,
          quality,
          reducedMotion,
        }).persistent,
        {
          view: viewRef.current,
          plane: planeRef.current,
          rectFor: (ref) => anchorsRef.current.get(ref),
        },
      ),
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
    // The §10.3 staging adapter (`./endpointOcclusion`): the effects layer can
    // see only whether a rect resolved, so the distinction between an endpoint
    // that is merely undrawn and one that has left the game is made HERE, where
    // both the authoritative view and the staged plane are in hand.
    const staged = (effects: GameViewPresentation['persistent']) =>
      attachOccludedEndpoints(effects, {
        view,
        plane,
        rectFor: (ref) => anchorsRef.current.get(ref),
      });

    // Persistent paths/links are a pure function of the current view — always
    // rebuilt from it alone so combat/targeting overlays are correct after any
    // discontinuity, never carried across a rebuild. `previous === undefined`
    // also means no §6.2 retraction survives a rebuild or a fast-forward: a
    // reconnect renders the settled stage (§6.4), and a superseding view is the
    // truth rather than something to animate up to.
    const currentPersistent = (): GameViewPresentation['persistent'] =>
      staged(
        deriveGameViewPresentation(undefined, view, {
          focusSeat: plane.focusSeat,
          ...(staging?.selectedId === undefined ? {} : { isolatedId: staging.selectedId }),
          targetingPaths,
          quality,
          reducedMotion,
        }).persistent,
      );

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
        // Focus isolates one object's relationships and calms the rest
        // (§4.4/§9.3): the selection is the isolation the player expressed.
        ...(staging?.selectedId === undefined ? {} : { isolatedId: staging.selectedId }),
        targetingPaths,
        quality,
        reducedMotion,
      });
      if (superseding) reconciler.discardMotionProxies();
      reconciler.reconcile(plane, presentation.motions);
      refreshVisualAnchors(anchorsRef.current, plane, reconciler, view);
      // A §6.2 retraction's endpoints have just left the view, so they are
      // frozen onto the rects they last occupied before the occlusion adapter
      // runs — that is where the player was already looking, and it is what
      // makes the retraction visible instead of instantly unresolvable.
      effectsLayer.setPersistent(
        staged(
          freezeDepartedRelationshipAnchors(
            presentation.persistent,
            previousAnchors,
            anchorsRef.current,
          ),
        ),
      );
      if (superseding) {
        effectsLayer.replaceTransients(
          freezeDepartedEffectAnchors(presentation.transients, previousAnchors, anchorsRef.current),
        );
      }
      startMotion();
      // The sound/haptic hooks (issue #507) subscribe to this same intent
      // stream. Fire-and-forget by contract: never awaited, never able to throw
      // out, and silent by default — the scene is already complete without it.
      presentAudio(presentation);
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
    // The isolation the relationship emphasis reads (§4.4). It only ever
    // changes together with `plane`, which is memoized on the same staging.
    staging?.selectedId,
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
      data-card-back={cardBack.skin?.id ?? 'procedural'}
      style={{ ...sceneStyle, ...cardBack.vars }}
      aria-label="2.5D battlefield"
    >
      {/* The card back (`card-representation.md` §13). One property for the
          whole plane, so every hidden pile shows the same image and none can
          vary with the card it hides. The probe exists because a CSS background
          cannot report a failure: it loads the same URL the piles paint (one
          request), and an error falls the device back to the default skin — or,
          if that is what failed, to the procedural back — with no layout
          change, because the pile's box never depended on the image. */}
      {cardBack.skin && (
        <img
          className={styles.cardBackProbe}
          src={cardBack.skin.src}
          alt=""
          aria-hidden="true"
          decoding="async"
          data-testid="card-back-probe"
          key={cardBack.skin.id}
          onError={() => noteCardBackFailed(cardBack.skin!.id)}
        />
      )}
      {/* ADR 0030 layer 1 — the shared environment (issue #530). The same
          component the pregame stage mounts, so crossing into the match never
          changes the world. Noninteractive, strictly behind the plane, and a
          pure function of (theme, viewport, quality, reduced motion): it never
          gates input and the match is fully interactive before it resolves. */}
      <SceneEnvironment
        quality={quality}
        reducedMotion={reducedMotion}
        viewport={{ width: plane.width, height: plane.height }}
        bias={environmentBias(plane)}
      />
      <div className={styles.camera}>
        <div className={styles.tiltedPlane}>
          <div
            ref={planeRootRef}
            className={`${styles.plane} ${clusterStyles.plane}`}
            data-testid="live-plane-dom"
          />
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
