/**
 * The effects surface mount (issue #482) — the successor of
 * `BattlefieldCanvas`'s canvas-owning role for the 2.5D client: a WebGL
 * overlay above the scene plane that hosts one {@link EffectsLayer}.
 *
 * Contract (ADR 0030 layer 3):
 * - **Passive**: `pointer-events: none`, `aria-hidden` — never a hit target,
 *   never announced; interactivity and reading live below/above it.
 * - **Zero idle cost**: the Pixi ticker runs only while the layer has live
 *   effects — the layer's `wake` callback restarts it, and it stops itself
 *   the moment an `advance` reports idle. No per-frame work while nothing is
 *   animating, no render passes on a clean surface.
 * - Pixi owns its own canvas; React owns only the container `<div>` (the
 *   carried StrictMode-safe mount shape). Where no WebGL context exists
 *   (jsdom/headless) the empty container stays silent — effects are
 *   decoration over state the view already applied, so nothing is lost.
 */
import { useEffect, useRef } from 'react';
import { Application } from 'pixi.js';
import { createEffectsTicker, type EffectsLayer } from './effects';

interface Props {
  /** The effects layer to host; the caller owns it and spawns into it. */
  layer: EffectsLayer;
  /** Logical surface size (the plane's size). */
  width: number;
  /** Logical surface height. */
  height: number;
}

export function EffectsSurface({ layer, width, height }: Props) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const appRef = useRef<Application | null>(null);

  useEffect(() => {
    const host = containerRef.current;
    if (!host) return;
    try {
      const app = new Application({
        backgroundAlpha: 0,
        antialias: true,
        autoDensity: true,
        resolution: window.devicePixelRatio || 1,
        width,
        height,
        // Render-on-demand: we drive rendering from the layer's activity;
        // Pixi must not repaint on its own schedule.
        autoStart: false,
      });
      const view = app.view as HTMLCanvasElement;
      view.style.display = 'block';
      host.appendChild(view);
      app.stage.addChild(layer.root);
      appRef.current = app;

      // Render-on-demand: each tick advances the layer (which draws only when
      // needed) and the ticker stops the moment no further frame is NEEDED —
      // a drawn static link or reduced-motion form is live but costs nothing
      // (`createEffectsTicker` carries the tested stop policy). The layer's
      // `wake` restarts the ticker on any new work — zero idle cost.
      const tick = createEffectsTicker(layer, {
        render: () => app.render(),
        stop: () => app.ticker.stop(),
      });
      app.ticker.add(() => tick(performance.now()));
      layer.wake = () => {
        if (!app.ticker.started) app.ticker.start();
      };
      layer.wake();
    } catch {
      // No WebGL (headless/jsdom): stay silent — effects are decoration only.
      appRef.current = null;
    }
    return () => {
      layer.wake = undefined;
      const app = appRef.current;
      if (app) {
        app.stage.removeChild(layer.root);
        app.destroy(true);
      }
      appRef.current = null;
    };
    // One app per mount; size changes re-mount via the key the caller sets.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [layer]);

  useEffect(() => {
    appRef.current?.renderer.resize(width, height);
  }, [width, height]);

  return (
    <div
      ref={containerRef}
      data-testid="effects-surface"
      aria-hidden="true"
      style={{ position: 'absolute', inset: 0, pointerEvents: 'none', lineHeight: 0 }}
    />
  );
}
