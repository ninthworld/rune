/**
 * The DOM plane's motion primitives (extracted from `planeReconciler.ts` when
 * it crossed the file-size ceiling — pure code motion, same behavior).
 *
 * Everything here is geometry and timing with no knowledge of staging: the
 * tween shapes the reconciler drives, the tokens' easing curve in JS form, the
 * layout-box writers, the semantic motion hint the presentation adapter (#492)
 * supplies, and the batch-stagger clamp of `presentation-budgets.md`
 * §Animation. Timestamps are always passed in — no wall clock lives here.
 */
import type { EntityId } from '../protocol';
import { SCENE_BATCH, SCENE_MOTION, sceneMotionMs } from '../sceneTokens';
import type { Rect } from './scene';

/** Resolved animation settings (durations seed from the #480 scene tokens). */
export interface PlaneAnimation {
  /** `prefers-reduced-motion`: every transition snaps; nothing else differs. */
  reducedMotion: boolean;
  /** Card move / travel-ghost duration (zone-travel class), ms. */
  travelMs: number;
  /** Region slot / focus re-staging duration (staging class), ms. */
  stagingMs: number;
  /** Per-item batch stagger, ms. */
  staggerMs: number;
  /** Total batch window, ms — items beyond it land together. */
  windowMs: number;
}

/** Optional semantic hint from the GameView presentation adapter (#492). */
export interface PlaneMotionHint {
  entityId?: EntityId;
  category: string;
  /** Semantic source anchor (`hand:*`, `stack:*`, `pile:*`, seat, or entity). */
  from?: string;
  /** Semantic destination anchor (`hand:*`, `stack:*`, `pile:*`, seat, or entity). */
  to?: string;
  durationMs: number;
  delayMs: number;
}

/** A decaying FLIP offset: the element's layout box is already at its final
 * rect; only this transform offset animates to zero. */
export interface FlipTween {
  el: HTMLElement;
  /** Offset from the final rect at tween start (the "invert" of FLIP). */
  dx: number;
  dy: number;
  duration: number;
  /** Batch-stagger delay before the tween begins, ms. */
  delay: number;
  /** First `advance` timestamp; set lazily so `reconcile` needs no clock. */
  start?: number;
}

/** A travel ghost: a decorative clone easing between two rects, then removed. */
export interface GhostTween {
  el: HTMLElement;
  from: Rect;
  to: Rect;
  /** Whether the ghost fades out as it travels (a leaving entity). */
  fadeOut: boolean;
  duration: number;
  delay: number;
  start?: number;
}

/** An entering wrapper's fade-up (opacity only; the box is already in place). */
export interface EnterTween {
  el: HTMLElement;
  duration: number;
  delay: number;
  start?: number;
}

/** Clamp to the unit interval. */
export function clamp01(t: number): number {
  return t < 0 ? 0 : t > 1 ? 1 : t;
}

/** Ease-out cubic — the JS form of the tokens' decelerate curve. */
export function easeOutCubic(t: number): number {
  const u = 1 - t;
  return 1 - u * u * u;
}

/** Apply a rect to an element's layout box (the authoritative position). */
export function applyRect(el: HTMLElement, rect: Rect): void {
  el.style.left = `${rect.x}px`;
  el.style.top = `${rect.y}px`;
  el.style.width = `${rect.w}px`;
  el.style.height = `${rect.h}px`;
}

/** Rect equality. */
export function sameRect(a: Rect, b: Rect): boolean {
  return a.x === b.x && a.y === b.y && a.w === b.w && a.h === b.h;
}

/** Lazily anchor a tween's clock and return its progress, or `null` before its
 * first `advance`. The batch-stagger delay holds progress at zero until it
 * elapses (so staggered items land inside the window, later ones together). */
export function progress(
  tween: { start?: number; delay: number; duration: number },
  now: number,
): number {
  if (tween.start === undefined) tween.start = now;
  if (tween.duration <= 0) return 1;
  return clamp01((now - tween.start - tween.delay) / tween.duration);
}

/** Apply the reconciler-owned FLIP translate to an authoritative rect. */
export function visualRect(rect: Rect, el: HTMLElement): Rect {
  const match = /translate\((-?[\d.]+)px, (-?[\d.]+)px\)/.exec(el.style.transform);
  if (!match) return rect;
  return { ...rect, x: rect.x + Number(match[1]), y: rect.y + Number(match[2]) };
}

/** Expose the active grammar class to CSS/card consumers; clear it on the next
 * view so a rapid update cannot leave stale semantic animation state behind. */
export function applyMotionHint(el: HTMLElement, hint: PlaneMotionHint | undefined): void {
  if (!hint) {
    delete el.dataset.motion;
    el.style.removeProperty('--motion-ms');
    el.style.removeProperty('--motion-delay-ms');
    el.style.removeProperty('--motion-card');
    el.style.removeProperty('--motion-card-delay');
    return;
  }
  el.dataset.motion = hint.category;
  el.style.setProperty('--motion-ms', `${hint.durationMs}ms`);
  el.style.setProperty('--motion-delay-ms', `${hint.delayMs}ms`);
  el.style.setProperty('--motion-card', `${hint.durationMs}ms`);
  el.style.setProperty('--motion-card-delay', `${hint.delayMs}ms`);
}

/** The batch-stagger delay for the `index`-th simultaneous item: per-item
 * stagger, clamped so every item completes inside the total window (items
 * beyond the window land together at its edge). */
export function batchDelay(index: number, anim: PlaneAnimation): number {
  return Math.min(index * anim.staggerMs, Math.max(0, anim.windowMs - anim.travelMs));
}

/** Resolve the `animate` option: token-seeded defaults, `null` when absent. */
export function resolvePlaneAnimation(
  animate: boolean | Partial<PlaneAnimation> | undefined,
): PlaneAnimation | null {
  if (!animate) return null;
  const defaults: PlaneAnimation = {
    reducedMotion: false,
    travelMs: SCENE_MOTION.zoneTravel.ms,
    stagingMs: SCENE_MOTION.staging.ms,
    staggerMs: SCENE_BATCH.staggerMs,
    windowMs: SCENE_BATCH.windowMs,
  };
  if (animate === true) return defaults;
  const resolved = { ...defaults, ...animate };
  // The reduced-motion collapse rides the tokens: zero duration ⇒ every
  // transition completes on its first advance with no intermediate state.
  if (resolved.reducedMotion) {
    resolved.travelMs = sceneMotionMs('zoneTravel', true);
    resolved.stagingMs = sceneMotionMs('staging', true);
  }
  return resolved;
}
