/**
 * The shared pregame stage (issue #506; `docs/design/front-door-and-lobby.md`
 * §5.0) — one continuous world behind Home, Lobby, and Room.
 *
 * Three layers, bottom to top:
 *
 * 1. **Environment** — the shared {@link SceneEnvironment} of issue #530
 *    (`docs/design/environment-system.md`): the ADR 0030 layer-1 L0–L3 stack the
 *    match mounts too, so there is one environment system and the crossing into
 *    the game has no boundary. It is mounted **once** and never re-mounts across
 *    a place change (§4.1): place changes move content, not the world. Layered
 *    SVG built from `sceneTokens.ts` — zero asset bytes against the ≤ 4 MB /
 *    ≤ 5 s load budgets, and the raster plates of #548 drop into the same slots.
 * 2. **Content** — the place's composition, in screen space.
 * 3. **Overlays** — settings and modals, at `SCENE_ELEVATION.screen`, rendered
 *    by the places themselves.
 *
 * The environment reads the shared quality tier and motion preference from the
 * device-local `presentationSettings` store exactly as the match's does — it is
 * literally the same component — and the content layer is never degraded at any
 * level. Every duration resolves through `pregameSceneVars`, so reduced motion
 * is a token-level `0`.
 *
 * Holds no game state: it renders whatever place its caller derived from the
 * store.
 */
import { useEffect, useRef, type ReactNode } from 'react';
import { SceneEnvironment } from '../table/environment';
import { useReducedMotion } from '../table/hooks/useReducedMotion';
import { usePresentationSettings } from '../table/settings/usePresentationSettings';
import { pregameSceneVars, type PregamePlace } from './pregameScene';
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
          identity survives every place change. It is the SAME environment
          component the match mounts (`table/environment`, issue #530), so the
          crossing into the game has no boundary and there is one environment
          system rather than two that drift. */}
      <div className={p.environment} data-testid="pregame-environment" aria-hidden="true">
        <SceneEnvironment quality={settings.quality} reducedMotion={reducedMotion} />
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
