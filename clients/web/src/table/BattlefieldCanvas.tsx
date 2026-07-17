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
 * Two invariants this component must uphold (both regressed once, issue #276):
 *  - **React owns the `<canvas>` element.** Pixi's `destroy(removeView)` detaches
 *    the view from the DOM when `removeView` is truthy; under React 18 StrictMode
 *    the mount→cleanup→mount cycle would then destroy the element React still
 *    thinks it owns, and every later frame draws into a detached node forever.
 *    Cleanup therefore passes `removeView = false` and lets React unmount the node.
 *  - **A canvas that genuinely fails is loud, not blank.** Where WebGL *is*
 *    available but the Pixi app cannot start or a frame throws, we surface a
 *    visible DOM fallback rather than an indistinguishable-from-broken blank
 *    board. Where WebGL is absent (jsdom/headless) the blank canvas is expected
 *    and stays silent, as it always has.
 */
import { useEffect, useRef, useState } from 'react';
import { Application, Container } from 'pixi.js';
import { SURFACES } from '../tokens';
import { SceneReconciler } from './sceneReconciler';
import type { TableScene } from './scene';

interface Props {
  /** The scene to draw; the canvas reconciles its display tree when it changes. */
  scene: TableScene;
}

/** `'#RRGGBB'` token to the numeric color Pixi expects. */
function hexToNumber(hex: string): number {
  return parseInt(hex.slice(1), 16);
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

export function BattlefieldCanvas({ scene }: Props) {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const appRef = useRef<Application | null>(null);
  const reconcilerRef = useRef<SceneReconciler | null>(null);
  // Set true only when WebGL is available yet the canvas still cannot render, so
  // the DOM fallback below appears instead of a silently blank board.
  const [renderFailed, setRenderFailed] = useState(false);

  // Create the Pixi application once, guarding against environments without a
  // real GL context (tests, SSR). The reconciler owns a root container parented
  // under the stage and lives exactly as long as the application.
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    try {
      const app = new Application({
        view: canvas,
        backgroundColor: hexToNumber(SURFACES.board),
        antialias: true,
        autoDensity: true,
        resolution: window.devicePixelRatio || 1,
        width: scene.width,
        height: scene.height,
      });
      const root = new Container();
      app.stage.addChild(root);
      appRef.current = app;
      reconcilerRef.current = new SceneReconciler(root);
      setRenderFailed(false);
    } catch {
      appRef.current = null;
      reconcilerRef.current = null;
      // Silent where WebGL simply isn't there (headless/jsdom); loud where it is.
      if (webglSupported()) setRenderFailed(true);
    }
    return () => {
      // `removeView = false`: the canvas is React-owned. Destroying with `true`
      // would detach the element React still tracks and break the next mount
      // (StrictMode double-invoke, issue #276). React removes the node on unmount.
      appRef.current?.destroy(false);
      appRef.current = null;
      reconcilerRef.current = null;
    };
    // Intentionally run once: the render effect below reacts to `scene`.
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
    } catch {
      // A frame threw on a GL-capable renderer: surface it instead of leaving a
      // blank board that reads as a broken game (issue #276).
      if (webglSupported()) setRenderFailed(true);
    }
  }, [scene]);

  return (
    <>
      {/*
       * Always rendered and always React-owned so unmount (and only unmount)
       * removes it. Hidden — not destroyed — while the DOM fallback stands in.
       */}
      <canvas
        ref={canvasRef}
        aria-hidden="true"
        style={{ display: renderFailed ? 'none' : 'block' }}
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
