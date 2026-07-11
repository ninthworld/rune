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
 */
import { useEffect, useRef } from 'react';
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

export function BattlefieldCanvas({ scene }: Props) {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const appRef = useRef<Application | null>(null);
  const reconcilerRef = useRef<SceneReconciler | null>(null);

  // Create the Pixi application once, guarding against environments without a
  // real GL context (tests, SSR). On failure the canvas stays blank and the DOM
  // overlay carries the entire experience. The reconciler owns a root container
  // parented under the stage and lives exactly as long as the application.
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
    } catch {
      appRef.current = null;
      reconcilerRef.current = null;
    }
    return () => {
      appRef.current?.destroy(true);
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
      // A broken/headless renderer: leave the canvas blank (see note above).
    }
  }, [scene]);

  return <canvas ref={canvasRef} aria-hidden="true" style={{ display: 'block' }} />;
}
