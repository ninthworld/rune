/**
 * The shared pregame stage (issue #506; `docs/design/front-door-and-lobby.md`
 * §5.0) — one continuous world behind Home, Lobby, and Room.
 *
 * Three layers, bottom to top:
 *
 * 1. **Environment** — the sky gradient, two ambient glows, far-ground
 *    silhouettes, and the arena edge, built from the same layer recipe as the
 *    match's backdrop (`table/live/live-plane.module.css`) over
 *    `SCENE_THEMES[DEFAULT_SCENE_THEME]`. It is mounted **once** and never
 *    re-mounts across a place change (§4.1): place changes move content, not the
 *    world, which is what makes the crossing into the match invisible. Pure CSS
 *    gradients — zero asset bytes against the ≤ 4 MB / ≤ 5 s load budgets.
 * 2. **Content** — the place's composition, in screen space.
 * 3. **Overlays** — settings and modals, at `SCENE_ELEVATION.screen`, rendered
 *    by the places themselves.
 *
 * The stage carries the shared quality tier as `data-environment="on | reduced
 * | off"`, driven by the device-local `presentationSettings` store exactly as
 * the match's environment is; the content layer is never degraded at any level.
 * Every duration resolves through `pregameSceneVars`, so reduced motion is a
 * token-level `0`.
 *
 * Holds no game state: it renders whatever place its caller derived from the
 * store.
 */
import { useEffect, useRef, type ReactNode } from 'react';
import { useReducedMotion } from '../table/hooks/useReducedMotion';
import { usePresentationSettings } from '../table/settings/usePresentationSettings';
import { pregameEnvironmentMotion, pregameSceneVars, type PregamePlace } from './pregameScene';
import p from './styles';

export interface PregameStageProps {
  /** Which place the content layer is showing (drives the staging transition). */
  place: PregamePlace;
  /** The place's composition. */
  children: ReactNode;
}

export function PregameStage({ place, children }: PregameStageProps) {
  const settings = usePresentationSettings();
  const reducedMotion = useReducedMotion(settings.motion);
  const contentRef = useRef<HTMLDivElement>(null);
  const previousPlace = useRef(place);

  // A place change moves focus to the new place's first heading (§5.10), so the
  // keyboard lands where the eye does. Deliberately skipped on first mount — a
  // cold start must not steal focus from the page.
  useEffect(() => {
    if (previousPlace.current === place) return;
    previousPlace.current = place;
    const heading = contentRef.current?.querySelector<HTMLElement>('[data-place-heading]');
    heading?.focus();
  }, [place]);

  return (
    <main className={p.stage} style={pregameSceneVars(reducedMotion)} data-testid="pregame-stage">
      {/* Layer 1 — the persistent world. Never keyed on `place`: this node's
          identity survives every place change. */}
      <div
        className={p.environment}
        data-environment={pregameEnvironmentMotion(settings.quality, reducedMotion)}
        data-testid="pregame-environment"
        aria-hidden="true"
      >
        <div className={p.sky} />
        <div className={p.ground} />
        <div className={p.arenaEdge} />
      </div>

      {/* Layer 2 — content. Keyed by place so the staging transition runs on a
          change; the animation is opacity/transform only and never gates input. */}
      <div className={p.content} ref={contentRef} data-place={place}>
        <div className={p.place} key={place}>
          {children}
        </div>
      </div>
    </main>
  );
}
