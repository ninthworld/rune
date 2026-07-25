/**
 * Scene-token wiring for the battlefield environment (issue #530,
 * `docs/design/environment-system.md` §5.4, §1.1, §8.2) — the `--env-*` custom
 * properties `environment.module.css` renders through.
 *
 * This is the `deck/deckScene.ts` / `pregame/pregameScene.ts` mould, unchanged:
 * **no new token values live here**. Every colour comes from `SCENE_THEMES`,
 * every duration from `SCENE_MOTION` through `sceneMotionMs`, so the stylesheet
 * carries no literal hex and no literal duration, and the reduced-motion
 * collapse is wired at the token level rather than in CSS.
 *
 * Pure functions over constants: no game logic, no I/O, no DOM.
 */
import type { CSSProperties } from 'react';
import { SCENE_MOTION, SCENE_THEMES, sceneMotionMs, type SceneThemeName } from '../../sceneTokens';
import { ENV_LAYERS } from './manifest';
import type { EnvironmentPlan } from './quality';

/** Custom-property style object usable as an inline `style`. */
export type EnvVars = CSSProperties & Record<`--${string}`, string | number>;

/**
 * The environment's scene-token custom properties for one theme: the thirteen
 * §5.4 palette slots, the four §1.1 parallax offsets already resolved to px for
 * the frame's excursion, and the `staging` motion class every layer cross-fade
 * and parallax tween runs on (§1.1, §8.2).
 *
 * Pass the composed `prefers-reduced-motion` result; the duration then collapses
 * to `0` and every cross-fade snaps, which is also the reconnect behaviour §7
 * rule 3 requires.
 */
export function environmentSceneVars(plan: EnvironmentPlan, reducedMotion: boolean): EnvVars {
  const theme = SCENE_THEMES[plan.theme];
  return {
    // §5.4 — the thirteen palette slots, by layer.
    '--env-surround-top': theme.surroundTop,
    '--env-surround-base': theme.surroundBase,
    '--env-water': theme.water,
    '--env-plaza-core': theme.plazaCore,
    '--env-plaza-edge': theme.plazaEdge,
    '--env-paving': theme.paving,
    '--env-medallion': theme.medallion,
    '--env-rim': theme.rim,
    '--env-verge': theme.verge,
    '--env-prop-warm': theme.propWarm,
    '--env-prop-cool': theme.propCool,
    '--env-glow': theme.glow,

    // §1.1 — the excursion and its per-layer offsets, already in px. Parallax is
    // driven only by the staging tween's plane delta; there is no free camera,
    // no pointer tracking, and no scroll input (ADR 0030).
    '--env-excursion': `${plan.excursionPx}px`,
    '--env-parallax-l0': `${ENV_LAYERS.l0.parallax * plan.excursionPx}px`,
    '--env-parallax-l1': `${ENV_LAYERS.l1.parallax * plan.excursionPx}px`,
    '--env-parallax-l2': `${ENV_LAYERS.l2.parallax * plan.excursionPx}px`,
    '--env-parallax-l3': `${ENV_LAYERS.l3.parallax * plan.excursionPx}px`,

    // §8.2 — each layer cross-fades in on the `staging` class; reduced motion
    // snaps. §7.1's ambient periods are derived from the same class so no
    // literal duration reaches the stylesheet.
    '--env-motion-staging': `${sceneMotionMs('staging', reducedMotion)}ms`,
    '--env-ease-staging': SCENE_MOTION.staging.ease,
  };
}

/**
 * The theme labels the settings surface offers, in token order. Display data
 * only — selecting one is a device preference and never touches the protocol.
 */
export function environmentThemeOptions(): { value: SceneThemeName; label: string }[] {
  return (Object.keys(SCENE_THEMES) as SceneThemeName[]).map((value) => ({
    value,
    label: SCENE_THEMES[value].label,
  }));
}
