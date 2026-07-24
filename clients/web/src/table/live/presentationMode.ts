/**
 * Production presentation modes for the live 2.5D match (issue #493).
 *
 * The scene distinguishes ordinary authoritative transitions from
 * *discontinuities* so a reconnect, resync, tab restore, or a burst of newer
 * views can never replay stale presentation. This module is pure: it decides
 * *which* mode a given view transition is, exposes the binding budgets, and
 * builds the single post-rebuild orientation cue. It holds no state and does no
 * I/O — the reconciler/effects orchestration lives in `LivePlane`.
 *
 * The four modes (scope of #493):
 * - `initial` — the first complete view mounts (motion suppressed); there is no
 *   previous scene to travel from.
 * - `reconcile` — the ordinary incremental path: one authoritative view follows
 *   another in the same transport session with presentation idle. Reserved for
 *   this common case (ADR 0030: full rebuilds are the exception).
 * - `rebuild` — a reconnect/resync discontinuity (a newer transport generation
 *   than the last presented view): rebuild the latest complete `GameView` with
 *   animation suppressed, after clearing every ghost, transient, path, cached
 *   anchor, and pending presentation intent.
 * - `fast-forward` — a newer view arrived before the prior transition settled:
 *   collapse to the newest view (skip the in-flight motion to its final layout,
 *   discard obsolete transient work) rather than queue gameplay behind
 *   animations.
 */
import type { GameView } from '../../protocol';
import { SCENE_HUES } from '../../sceneTokens';
import type { TransientInvocation } from '../effects';

/** The production presentation mode for one authoritative view transition. */
export type PresentationMode = 'initial' | 'reconcile' | 'rebuild' | 'fast-forward';

/** Pure inputs that classify a view transition. */
export interface PresentationModeInput {
  /** Whether a previous view has already been presented on this mount. */
  hasPreviousView: boolean;
  /**
   * Whether the transport generation advanced since the last presented view —
   * the reconnect/resync/tab-restore signal (`GameStore.sessionEpoch`).
   */
  discontinuity: boolean;
  /**
   * Whether the prior transition is still animating as this newer view arrives
   * (`PlaneReconciler.hasPendingAnimations()`), i.e. views are arriving faster
   * than the composition can present them.
   */
  presentationBusy: boolean;
}

/**
 * Classify a view transition. A discontinuity always wins over a busy
 * composition: a reconnect frame is rebuilt from scratch even mid-animation, so
 * no pre-disconnect motion can survive into the rebuilt scene.
 */
export function determinePresentationMode(input: PresentationModeInput): PresentationMode {
  if (!input.hasPreviousView) return 'initial';
  if (input.discontinuity) return 'rebuild';
  if (input.presentationBusy) return 'fast-forward';
  return 'reconcile';
}

/** Whether a mode tears the scene down and rebuilds it from the latest view alone. */
export function isFullRebuild(mode: PresentationMode): boolean {
  return mode === 'initial' || mode === 'rebuild';
}

/**
 * The reconnect / fast-forward full-scene-rebuild budgets, in ms
 * (`docs/design/presentation-budgets.md` §Performance budgets). Compact
 * (tablet/phone tier) gets the looser ceiling.
 */
export const REBUILD_BUDGET_MS = { desktop: 50, compact: 100 } as const;

/** The scene DOM ceiling — total reconciled scene nodes (budgets §Performance). */
export const SCENE_DOM_CEILING = 15_000;

/**
 * Duration of the post-rebuild "you are here" pulse, in ms (visual-system §8:
 * "pulse ≤ 300 ms"). Kept in sync with the `orientPulse` keyframe in
 * `live-match.module.css`.
 */
export const ORIENTATION_PULSE_MS = 300;

/** Pick the rebuild budget for the current composition tier. */
export function rebuildBudgetMs(compact: boolean): number {
  return compact ? REBUILD_BUDGET_MS.compact : REBUILD_BUDGET_MS.desktop;
}

/** One measured rebuild/fast-forward, for the CI-runnable budget harness (#493). */
export interface RebuildSample {
  /** The mode that produced this sample (a full rebuild or a collapse). */
  mode: PresentationMode;
  /** Wall-clock milliseconds the DOM rebuild/collapse took. */
  durationMs: number;
  /** Reconciled scene DOM node count after the rebuild. */
  domNodes: number;
  /** The budget the duration is measured against (desktop/compact tier). */
  budgetMs: number;
  /** Whether the rebuild landed inside its time budget. */
  withinBudget: boolean;
  /** Whether the rebuilt scene stayed under the DOM ceiling. */
  withinDomCeiling: boolean;
}

/**
 * The single non-blocking "you are here" cue shown after an animated-mode
 * rebuild (`docs/design/visual-system.md` §8 Session moments — "latest view
 * renders complete, then a single 'you are here' pulse on the phase pill and
 * active crest"). One `flow` pulse lands on the active player's crest; the
 * phase-pill half is a chrome affordance driven separately. Reduced motion
 * receives the complete final state with **no** pulse.
 */
export function orientationCue(view: GameView, reducedMotion: boolean): TransientInvocation[] {
  if (reducedMotion || !view.active_player) return [];
  return [
    {
      category: 'flow',
      target: { ref: `seat:${view.active_player}` },
      accent: SCENE_HUES.gold.value,
    },
  ];
}
