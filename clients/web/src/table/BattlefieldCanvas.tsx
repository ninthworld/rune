/**
 * The Pixi canvas layer (ADR 0003: battlefield + hand cards live in the canvas).
 *
 * It is a pure *visual* surface: it draws the cards the {@link TableScene}
 * describes with the shared card factory and nothing else. All interactivity —
 * selection, action confirmation — lives in the DOM overlay above it, so this
 * component never needs to be exercised for the routing tests, and it degrades
 * to a no-op where no real WebGL context exists (headless CI / jsdom).
 */
import { useEffect, useRef } from 'react';
import { Application, Container } from 'pixi.js';
import { buildCardDisplay } from '../card/cardFactory';
import { SURFACES } from '../tokens';
import type { RenderedCard, TableScene } from './scene';

interface Props {
  /** The scene to draw; the canvas re-renders wholesale when it changes. */
  scene: TableScene;
}

/** `'#RRGGBB'` token to the numeric color Pixi expects. */
function hexToNumber(hex: string): number {
  return parseInt(hex.slice(1), 16);
}

/** Add one card's display object to the stage at its scene rect. */
function placeCard(root: Container, card: RenderedCard): void {
  const display = buildCardDisplay(card.data, card.tier);
  display.position.set(card.rect.x, card.rect.y);
  root.addChild(display);
}

export function BattlefieldCanvas({ scene }: Props) {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const appRef = useRef<Application | null>(null);

  // Create the Pixi application once, guarding against environments without a
  // real GL context (tests, SSR). On failure the canvas stays blank and the DOM
  // overlay carries the entire experience.
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    try {
      appRef.current = new Application({
        view: canvas,
        backgroundColor: hexToNumber(SURFACES.board),
        antialias: true,
        autoDensity: true,
        resolution: window.devicePixelRatio || 1,
        width: scene.width,
        height: scene.height,
      });
    } catch {
      appRef.current = null;
    }
    return () => {
      appRef.current?.destroy(true);
      appRef.current = null;
    };
    // Intentionally run once: the render effect below reacts to `scene`.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Redraw the stage from the current scene. Fully deterministic: identical
  // scenes yield identical draws (reconnect/replay invariant).
  useEffect(() => {
    const app = appRef.current;
    if (!app) return;
    try {
      app.renderer.resize(scene.width, scene.height);
      app.stage.removeChildren();
      const root = new Container();
      for (const band of scene.bands) for (const card of band.cards) placeCard(root, card);
      for (const card of scene.hand) placeCard(root, card);
      app.stage.addChild(root);
    } catch {
      // A broken/headless renderer: leave the canvas blank (see note above).
    }
  }, [scene]);

  return <canvas ref={canvasRef} aria-hidden="true" style={{ display: 'block' }} />;
}
