import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
} from 'react';
import { CardFace } from '../card/dom';
import { SCENE_ELEVATION, SCENE_HUES, SCENE_NEUTRALS, SCENE_SEAT_ACCENTS } from '../sceneTokens';
import { stagePlane, type PlaneRegion, type StagedPlane } from '../table/plane';
import { cardFaceRenderer } from '../table/planeFaceRenderer';
import { domCardArt, handDisplayData, planeDisplayData } from '../table/planeDisplayData';
import { PlaneReconciler, planeRegions, planeRenders } from '../table/planeReconciler';
import { EffectsLayer, type EffectDensity, type EffectQuality } from '../table/effects';
import { EffectsSurface } from '../table/EffectsSurface';
import { SceneEnvironment } from '../table/environment';
import { FIXTURE_SCENARIOS, fixtureScenario, type FixtureScenario } from './scenarios';
import {
  FrameBudgetSampler,
  fixtureBudgetReport,
  maxCardFaceNodes,
  type FixtureBudgetReport,
} from './metrics';
import styles from './fixture-battlefield.module.css';

/** Dev/test-only window surface used by screenshot and budget automation. */
export interface FixtureHarnessHook {
  /** True after the plane, effects layer, and first report are mounted. */
  ready: boolean;
  /** Current measurement report. */
  report: FixtureBudgetReport;
  /** Select one of {@link FIXTURE_SCENARIOS}. */
  selectScenario(id: string): void;
  /** Select an authoritative frame by zero-based index. */
  selectFrame(index: number): void;
  /** Start the current sequence. */
  play(): void;
  /** Pause the current sequence. */
  pause(): void;
  /** Measure a reconnect-style full scene rebuild. */
  rebuild(): number;
}

declare global {
  interface Window {
    /** Present only on the gated `/fixtures/2.5d` route. */
    __RUNE_2_5D_FIXTURE__?: FixtureHarnessHook;
  }
}

type SceneStyle = CSSProperties & Record<`--${string}`, string | number>;

const harnessStyle: SceneStyle = {
  '--ink': SCENE_NEUTRALS.ink,
  '--surface-base': SCENE_NEUTRALS.surfaceBase,
  '--raised': SCENE_NEUTRALS.raised,
  '--line-strong': SCENE_NEUTRALS.lineStrong,
  '--text': SCENE_NEUTRALS.text,
  '--gold': SCENE_HUES.gold.value,
  '--blue': SCENE_HUES.blue.value,
  '--orange': SCENE_HUES.orange.value,
  '--seat-azure': SCENE_SEAT_ACCENTS[0],
  '--seat-amethyst': SCENE_SEAT_ACCENTS[3],
  '--seat-teal': SCENE_SEAT_ACCENTS[5],
  '--shadow-rest': SCENE_ELEVATION.rest.shadow,
  '--shadow-screen': SCENE_ELEVATION.screen.shadow,
};

function initialScenario(): FixtureScenario {
  if (typeof window === 'undefined') return FIXTURE_SCENARIOS[0]!;
  return fixtureScenario(new URLSearchParams(window.location.search).get('scenario'));
}

function initialFrame(): number {
  if (typeof window === 'undefined') return 0;
  const value = Number(new URLSearchParams(window.location.search).get('frame') ?? 0);
  return Number.isInteger(value) && value >= 0 ? value : 0;
}

function initialQuality(): EffectQuality {
  if (typeof window === 'undefined') return 'standard';
  const value = new URLSearchParams(window.location.search).get('quality');
  return value === 'high' || value === 'lite' ? value : 'standard';
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

/** Fixture-driven Phase 1 2.5D integration battlefield (issue #483). */
export function FixtureBattlefield() {
  const [scenarioId, setScenarioId] = useState(() => initialScenario().id);
  const [frameIndex, setFrameIndex] = useState(initialFrame);
  const [quality, setQuality] = useState<EffectQuality>(initialQuality);
  const [density, setDensity] = useState<EffectDensity>('reduced');
  const [reducedMotion, setReducedMotion] = useState(false);
  const [playing, setPlaying] = useState(false);
  const [scale, setScale] = useState(1);
  const [rebuildMs, setRebuildMs] = useState(0);
  const [faceNodes, setFaceNodes] = useState(0);
  const [, setReportTick] = useState(0);

  const scenario = fixtureScenario(scenarioId);
  const safeFrameIndex = Math.min(frameIndex, scenario.frames.length - 1);
  const frame = scenario.frames[safeFrameIndex]!;
  const compact = scenario.viewport.height > scenario.viewport.width;
  const captureMode =
    typeof window !== 'undefined' &&
    new URLSearchParams(window.location.search).get('capture') === '1';
  const plane = useMemo(
    () => stagePlane(frame.view, scenario.viewport, frame.staging),
    [frame, scenario.viewport],
  );

  const stageHostRef = useRef<HTMLDivElement | null>(null);
  const planeRootRef = useRef<HTMLDivElement | null>(null);
  const reconcilerRef = useRef<PlaneReconciler | null>(null);
  const frameRef = useRef(frame);
  const planeRef = useRef(plane);
  const anchorsRef = useRef(new Map<string, { x: number; y: number; w: number; h: number }>());
  const samplerRef = useRef(new FrameBudgetSampler());
  const transientKeyRef = useRef('');
  frameRef.current = frame;
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

  const report = fixtureBudgetReport({
    scenario: scenario.id,
    quality,
    compact,
    idle: samplerRef.current.idleSummary(),
    tween: samplerRef.current.tweenSummary(),
    rebuildMs,
    domNodes: typeof document === 'undefined' ? 0 : document.querySelectorAll('*').length,
    faceNodes,
  });

  const selectScenario = useCallback((id: string) => {
    const next = fixtureScenario(id);
    setScenarioId(next.id);
    setFrameIndex(0);
    setPlaying(false);
    samplerRef.current.reset();
    if (typeof window !== 'undefined') {
      const url = new URL(window.location.href);
      url.searchParams.set('scenario', next.id);
      window.history.replaceState(null, '', url);
    }
  }, []);

  const selectFrame = useCallback(
    (index: number) => {
      const last = scenario.frames.length - 1;
      setFrameIndex(Math.max(0, Math.min(last, index)));
    },
    [scenario.frames.length],
  );

  const rebuild = useCallback((): number => {
    const reconciler = reconcilerRef.current;
    if (!reconciler) return 0;
    const started = performance.now();
    reconciler.rebuild(planeRef.current);
    const elapsed = performance.now() - started;
    anchorPlane(anchorsRef.current, planeRef.current, reconciler);
    setRebuildMs(elapsed);
    setFaceNodes(maxCardFaceNodes(reconciler.root));
    return elapsed;
  }, []);

  useLayoutEffect(() => {
    const root = planeRootRef.current;
    if (!root) return;
    root.replaceChildren();
    const reconciler = new PlaneReconciler(root, {
      face: cardFaceRenderer((render) =>
        planeDisplayData(frameRef.current.view, frameRef.current.staging, render),
      ),
      animate: { reducedMotion },
    });
    reconcilerRef.current = reconciler;
    const started = performance.now();
    reconciler.rebuild(planeRef.current);
    setRebuildMs(performance.now() - started);
    anchorPlane(anchorsRef.current, planeRef.current, reconciler);
    setFaceNodes(maxCardFaceNodes(root));
    return () => {
      reconciler.clear();
      root.replaceChildren();
      if (reconcilerRef.current === reconciler) reconcilerRef.current = null;
    };
  }, [reducedMotion]);

  useLayoutEffect(() => {
    const reconciler = reconcilerRef.current;
    if (!reconciler) return;
    reconciler.reconcile(plane);
    anchorPlane(anchorsRef.current, plane, reconciler);
    setFaceNodes(maxCardFaceNodes(reconciler.root));
    effectsLayer.setPersistent(frame.effects ?? []);
    const transientKey = `${scenario.id}:${safeFrameIndex}`;
    if (frame.transient && transientKeyRef.current !== transientKey) {
      effectsLayer.spawn(frame.transient);
    }
    transientKeyRef.current = transientKey;
  }, [effectsLayer, frame, plane, safeFrameIndex, scenario.id]);

  useLayoutEffect(() => {
    const host = stageHostRef.current;
    if (!host) return;
    const resize = (): void => {
      const bounds = host.getBoundingClientRect();
      if (bounds.width <= 0 || bounds.height <= 0) return;
      setScale(
        Math.min(bounds.width / scenario.viewport.width, bounds.height / scenario.viewport.height),
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
  }, [scenario.viewport.height, scenario.viewport.width]);

  useEffect(() => {
    let raf = 0;
    let sampled = 0;
    const tick = (now: number): void => {
      const reconciler = reconcilerRef.current;
      const tweening = reconciler?.hasPendingAnimations() ?? false;
      reconciler?.advance(now);
      effectsLayer.trackMotion(tweening);
      samplerRef.current.sample(now, tweening ? 'tween' : 'idle');
      sampled += 1;
      if (sampled % 15 === 0) setReportTick((value) => value + 1);
      raf = window.requestAnimationFrame(tick);
    };
    raf = window.requestAnimationFrame(tick);
    return () => window.cancelAnimationFrame(raf);
  }, [effectsLayer]);

  useEffect(() => {
    if (!playing || scenario.frames.length < 2) return;
    const timer = window.setInterval(() => {
      setFrameIndex((index) => (index + 1) % scenario.frames.length);
    }, 900);
    return () => window.clearInterval(timer);
  }, [playing, scenario.frames.length]);

  useEffect(() => {
    window.__RUNE_2_5D_FIXTURE__ = {
      ready: reconcilerRef.current !== null,
      report,
      selectScenario,
      selectFrame,
      play: () => setPlaying(true),
      pause: () => setPlaying(false),
      rebuild,
    };
    return () => {
      delete window.__RUNE_2_5D_FIXTURE__;
    };
  }, [rebuild, report, selectFrame, selectScenario]);

  const logicalStyle: SceneStyle = {
    width: scenario.viewport.width,
    height: scenario.viewport.height,
    transform: `translate(-50%, -50%) scale(${scale})`,
    '--scene-width': `${scenario.viewport.width}px`,
    '--scene-height': `${scenario.viewport.height}px`,
  };

  return (
    <main
      className={styles.harness}
      data-testid="fixture-battlefield"
      data-capture={String(captureMode)}
      style={harnessStyle}
    >
      <header className={styles.toolbar}>
        <div className={styles.titleBlock}>
          <span className={styles.eyebrow}>Phase 1 integration fixture</span>
          <h1>RUNE 2.5D battlefield</h1>
          <p>{scenario.description}</p>
        </div>
        <div className={styles.controls}>
          <label>
            Scenario
            <select
              aria-label="Fixture scenario"
              value={scenario.id}
              onChange={(event) => selectScenario(event.target.value)}
            >
              {FIXTURE_SCENARIOS.map((entry) => (
                <option key={entry.id} value={entry.id}>
                  {entry.label}
                </option>
              ))}
            </select>
          </label>
          <label>
            Quality
            <select
              aria-label="Effect quality"
              value={quality}
              onChange={(event) => setQuality(event.target.value as EffectQuality)}
            >
              <option value="high">High</option>
              <option value="standard">Standard</option>
              <option value="lite">Lite</option>
            </select>
          </label>
          <label>
            Effects
            <select
              aria-label="Effect density"
              value={density}
              onChange={(event) => setDensity(event.target.value as EffectDensity)}
            >
              <option value="full">Full</option>
              <option value="reduced">Reduced</option>
              <option value="minimal">Minimal</option>
            </select>
          </label>
          <label className={styles.check}>
            <input
              type="checkbox"
              checked={reducedMotion}
              onChange={(event) => setReducedMotion(event.target.checked)}
            />
            Reduced motion
          </label>
        </div>
      </header>

      <section
        ref={stageHostRef}
        className={styles.stageViewport}
        data-compact={String(compact)}
        aria-label={`${scenario.label}: ${frame.label}`}
      >
        <div
          className={styles.logicalStage}
          style={logicalStyle}
          data-reduced-motion={String(reducedMotion)}
        >
          {/* The shared ADR 0030 layer-1 environment (issue #530) — the same
              component the match and the pregame stage mount, measured against
              the scenario's logical viewport so the harness reproduces the crop
              and excursion the real client would pick at that geometry. */}
          <SceneEnvironment
            quality={quality}
            reducedMotion={reducedMotion}
            viewport={scenario.viewport}
          />
          <div className={styles.camera}>
            <div className={styles.tiltedPlane}>
              <div ref={planeRootRef} className={styles.plane} data-testid="fixture-plane" />
              <EffectsSurface
                key={`${quality}:${density}:${String(reducedMotion)}`}
                layer={effectsLayer}
                width={scenario.viewport.width}
                height={scenario.viewport.height}
              />
            </div>
          </div>

          <div className={styles.phasePill}>
            <span>Turn {frame.view.turn || 5}</span>
            <strong>{frame.view.phase.replaceAll('_', ' ')}</strong>
            <span>
              {frame.view.player_names[frame.view.priority_player ?? ''] ?? 'No priority'}
            </span>
          </div>

          <aside className={styles.stackRail} data-open={String(frame.view.stack.length > 0)}>
            <span className={styles.railTitle}>Stack · {frame.view.stack.length}</span>
            {frame.view.stack
              .slice()
              .reverse()
              .map((item, index) => (
                <div
                  className={styles.stackItem}
                  key={item.id}
                  style={{ '--stack-offset': `${index}px` } as SceneStyle}
                >
                  <span>{frame.view.player_names[item.controller] ?? item.controller}</span>
                  <strong>{item.description}</strong>
                </div>
              ))}
          </aside>

          <div className={styles.handFan} data-count={frame.view.my_hand.length}>
            {frame.view.my_hand.map((entry, index) => {
              const center = (frame.view.my_hand.length - 1) / 2;
              return (
                <div
                  className={styles.handCard}
                  key={entry.id}
                  style={
                    {
                      '--fan-x': `${(index - center) * 44}px`,
                      '--fan-y': `${Math.abs(index - center) * 2.2}px`,
                      '--fan-angle': `${(index - center) * 2.4}deg`,
                      '--fan-depth': index,
                    } as SceneStyle
                  }
                >
                  <CardFace
                    data={handDisplayData(frame.view, entry)}
                    tier="hand"
                    art={domCardArt(entry)}
                    rulesText={entry.rules_text}
                  />
                </div>
              );
            })}
          </div>

          <div className={styles.actionDock}>
            <span>{frame.label}</span>
            <span className={styles.dockAction}>
              {frame.view.valid_actions.find((action) => !action.subject?.length)?.label ??
                'No action offered'}
            </span>
          </div>
        </div>
      </section>

      <footer className={styles.transport}>
        <div className={styles.sequenceControls}>
          <button type="button" onClick={() => selectFrame(safeFrameIndex - 1)}>
            Previous
          </button>
          <button type="button" onClick={() => setPlaying((value) => !value)}>
            {playing ? 'Pause sequence' : 'Play sequence'}
          </button>
          <button type="button" onClick={() => selectFrame(safeFrameIndex + 1)}>
            Next
          </button>
          <button type="button" onClick={() => reconcilerRef.current?.skipTransitions()}>
            Skip motion
          </button>
          <button type="button" onClick={rebuild}>
            Rebuild
          </button>
          <span>
            {safeFrameIndex + 1}/{scenario.frames.length} · {frame.label}
          </span>
        </div>
        <dl className={styles.metrics} data-pass={String(report.passes)}>
          <div>
            <dt>Idle</dt>
            <dd>{report.idle.fps.toFixed(1)} fps</dd>
          </div>
          <div>
            <dt>Tween</dt>
            <dd>{report.tween.fps.toFixed(1)} fps</dd>
          </div>
          <div>
            <dt>p95</dt>
            <dd>{Math.max(report.idle.p95Ms, report.tween.p95Ms).toFixed(1)} ms</dd>
          </div>
          <div>
            <dt>Rebuild</dt>
            <dd>{report.rebuildMs.toFixed(2)} ms</dd>
          </div>
          <div>
            <dt>DOM</dt>
            <dd>{report.domNodes.toLocaleString()} nodes</dd>
          </div>
          <div>
            <dt>Face</dt>
            <dd>{report.faceNodes} nodes</dd>
          </div>
        </dl>
      </footer>
    </main>
  );
}
