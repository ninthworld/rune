/**
 * The Pixi canvas layer (ADR 0003: battlefield + hand cards live in the canvas).
 *
 * It is a pure *visual* surface: it draws the cards the {@link TableScene}
 * describes with the shared card factory and nothing else. All interactivity —
 * selection, action confirmation — lives in the DOM overlay above it, so this
 * component never needs to be exercised for the routing tests, and it degrades
 * to a no-op where no real WebGL context exists (headless CI / jsdom).
 *
 * Scene updates are *reconciled* by entity id through a {@link SceneReconciler}
 * (issue #58): unchanged cards are reused and repositioned in place rather than
 * the whole scene graph being rebuilt each frame. The reconcile cache is a pure
 * optimization — a fresh mount of any single scene renders identically.
 *
 * **Pixi owns its own canvas; React owns only the container `<div>`** (issue #276).
 * This is the crux of the StrictMode story: React 18 runs every effect
 * mount→cleanup→mount in dev. If React owned the `<canvas>` and we merely avoided
 * detaching it (an earlier fix), the first app's cleanup still destroys that
 * canvas's WebGL context, and the second mount cannot obtain a *fresh* context
 * from the same element — Pixi throws and the board never paints. So instead we
 * let `new Application()` create its own canvas, append it to the React-owned
 * container, and on cleanup `destroy(true)` disposes the app **and its own
 * canvas**. Each mount therefore gets a brand-new canvas with a brand-new
 * context, and React's container is never touched by Pixi.
 *
 * When WebGL *is* available but Pixi still cannot start (or a frame throws), a
 * visible DOM fallback stands in rather than a silently blank board — a blank
 * board is indistinguishable from a broken game. Where WebGL is absent
 * (jsdom/headless) the empty container stays silent, as it always has.
 */
import { useEffect, useRef, useState } from 'react';
import { Application, Container } from 'pixi.js';
import { SURFACES } from '../tokens';
import { SceneReconciler } from './sceneReconciler';
import type { TableScene } from './scene';
import type { EntityId } from '../protocol';

interface Props {
  /** The scene to draw; the canvas reconciles its display tree when it changes. */
  scene: TableScene;
  /** The focused/selected/hovered participant whose combat links are isolated on a
   * crowded board (issue #339). `null`/absent draws every link. Ephemeral. */
  isolatedId?: EntityId | null;
}

/** `'#RRGGBB'` token to the numeric color Pixi expects. */
function hexToNumber(hex: string): number {
  return parseInt(hex.slice(1), 16);
}

/**
 * Whether the viewer asked for reduced motion (issue #334). A hard accessibility
 * requirement: when true the reconciler snaps every diff instantly, with no layout
 * or state difference from the animated path. Guarded for environments without
 * `matchMedia` (older jsdom / SSR), where it degrades to "no reduced-motion request".
 */
function prefersReducedMotion(): boolean {
  try {
    return window.matchMedia('(prefers-reduced-motion: reduce)').matches;
  } catch {
    return false;
  }
}

/**
 * Whether the environment can provide a WebGL context at all. Used to decide
 * whether a Pixi failure is a *real* rendering fault worth surfacing to the user
 * or the expected headless/jsdom no-op that must stay silent (see file header).
 */
function webglSupported(): boolean {
  try {
    const probe = document.createElement('canvas');
    return Boolean(probe.getContext('webgl2') || probe.getContext('webgl'));
  } catch {
    return false;
  }
}

export function BattlefieldCanvas({ scene, isolatedId = null }: Props) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const appRef = useRef<Application | null>(null);
  const reconcilerRef = useRef<SceneReconciler | null>(null);
  // Set true only when WebGL is available yet the canvas still cannot render, so
  // the DOM fallback below appears instead of a silently blank board.
  const [renderFailed, setRenderFailed] = useState(false);

  // Create the Pixi application once per mount, guarding against environments
  // without a real GL context (tests, SSR). Pixi creates and owns its canvas; we
  // parent it under the React-owned container. The reconciler owns a root
  // container under the stage and lives exactly as long as the application.
  useEffect(() => {
    const host = containerRef.current;
    if (!host) return;
    try {
      const app = new Application({
        // Transparent: the table surface (vignette + rune motif, DOM beneath the
        // canvas) shows through — the board is drawn ON the table, not over it.
        backgroundAlpha: 0,
        backgroundColor: hexToNumber(SURFACES.board),
        antialias: true,
        autoDensity: true,
        resolution: window.devicePixelRatio || 1,
        width: scene.width,
        height: scene.height,
      });
      const view = app.view as HTMLCanvasElement;
      view.style.display = 'block';
      host.appendChild(view);
      const root = new Container();
      app.stage.addChild(root);
      appRef.current = app;
      // Animate the view diff (issue #334): cards ease between layouts, entering ones
      // fade up, leaving ones fade out — honoring reduced motion, which snaps instead.
      const reconciler = new SceneReconciler(root, {
        animate: { reducedMotion: prefersReducedMotion() },
      });
      reconcilerRef.current = reconciler;
      reconciler.reconcile(scene);
      // Drive the transitions from the Pixi ticker (before its own render pass). This
      // only moves pixels toward layouts the scene already made authoritative, so it
      // never gates input — a live prompt is actionable the instant its view arrives.
      app.ticker.add(() => reconciler.advance(performance.now()));
      setRenderFailed(false);
    } catch {
      appRef.current = null;
      reconcilerRef.current = null;
      // Silent where WebGL simply isn't there (headless/jsdom); loud where it is.
      if (webglSupported()) setRenderFailed(true);
    }
    return () => {
      // Destroy the app AND its own canvas (`removeView = true`). The canvas is
      // Pixi's element, not React's, so removing it is correct — and it means the
      // next mount starts from a fresh canvas with a fresh GL context rather than
      // trying (and failing) to reuse a context-less one (issue #276).
      appRef.current?.destroy(true);
      appRef.current = null;
      reconcilerRef.current = null;
    };
    // Intentionally run once per mount: the render effect below reacts to `scene`.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Reconcile the display tree to the current scene by entity id. Deterministic:
  // after any single scene the tree matches a fresh mount (reconnect/replay
  // invariant); the reconcile cache only avoids rebuilding unchanged cards.
  useEffect(() => {
    const app = appRef.current;
    const reconciler = reconcilerRef.current;
    if (!app || !reconciler) return;
    try {
      app.renderer.resize(scene.width, scene.height);
      reconciler.reconcile(scene);
      reconciler.setIsolation(isolatedId);
    } catch {
      // A frame threw on a GL-capable renderer: surface it instead of leaving a
      // blank board that reads as a broken game (issue #276).
      if (webglSupported()) setRenderFailed(true);
    }
  }, [scene, isolatedId]);

  return (
    <>
      {/*
       * React owns only this container; Pixi appends and removes its own canvas
       * inside it. Hidden — not unmounted — while the DOM fallback stands in, so
       * the mount effect's ref stays valid.
       */}
      <div
        ref={containerRef}
        data-testid="battlefield-canvas-host"
        aria-hidden="true"
        style={{ display: renderFailed ? 'none' : 'block', lineHeight: 0 }}
      />
      {renderFailed && <BoardRenderFallback scene={scene} />}
    </>
  );
}

/** Card-body surface, spelled out so the fallback reads against the board color. */
const fallbackStyle: React.CSSProperties = {
  position: 'absolute',
  inset: 0,
  display: 'flex',
  flexDirection: 'column',
  gap: 8,
  padding: 16,
  overflow: 'auto',
  background: SURFACES.board,
  color: SURFACES.nameText,
  font: '14px system-ui, sans-serif',
};

/**
 * Visible stand-in for a board that could not render on a GL-capable client
 * (issue #276). A dumb renderer whose renderer silently no-ops is
 * indistinguishable from a broken game, so we say so and still list the cards in
 * play — the DOM overlay's inspect handles above continue to work over this.
 */
function BoardRenderFallback({ scene }: { scene: TableScene }) {
  const battlefield = scene.bands.flatMap((band) => band.cards.map((c) => c.name));
  return (
    <div role="alert" data-testid="board-render-fallback" style={fallbackStyle}>
      <strong>Board rendering failed.</strong>
      <span style={{ color: SURFACES.typeText }}>
        The card canvas could not start on this device. Cards in play are listed below; the inspect
        handles above still work.
      </span>
      <div>
        <strong>Battlefield:</strong> {battlefield.length ? battlefield.join(', ') : '—'}
      </div>
      <div>
        <strong>Hand:</strong> {scene.hand.length ? scene.hand.map((c) => c.name).join(', ') : '—'}
      </div>
    </div>
  );
}
