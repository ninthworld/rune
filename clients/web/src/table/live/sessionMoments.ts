/**
 * Session moments in the motion grammar (issue #509) —
 * `docs/design/visual-system.md` §8 "Session moments".
 *
 * The moments that open and close a game: game start, the mulligan, the
 * reconnect acknowledgment, the verdict (victory / defeat / draw / concede),
 * and the exit back to the lobby. This module is the one place that names them,
 * carries their budgets, and classifies which one a given presentation
 * transition is. It is **pure**: no state, no timers, no DOM, no I/O — the
 * chrome hooks own the clocks and the adapter owns the intents.
 *
 * Three contracts bind every row here, and every consumer:
 *
 * - **Never gates input.** A moment is opacity/transform/effect staging over a
 *   view that is already authoritative and already interactive. Hit targets
 *   exist at their final rects the moment a scene is built
 *   (`presentation-budgets.md` §Performance, hard rules).
 * - **Interruptible.** A newer authoritative view retargets or discards any
 *   in-flight moment; nothing here queues gameplay behind animation.
 * - **Skippable where the grammar marks it.** Compositions that may exceed
 *   {@link SCENE_SKIP_THRESHOLD_MS} (game start, victory, the mulligan
 *   sweep+deal) end early on any deliberate input. Reduced motion snaps
 *   straight to the end state — {@link momentDurationMs} returns 0 there, so
 *   nothing downstream needs its own collapse.
 *
 * Restraint is a requirement, not a preference (§2 semantic hue families): a
 * verdict wears the **loss moment** (red) or **gain moment** family, victory
 * gets the disciplined gold bloom, and there is no confetti.
 */
import type { GameResult, PlayerId } from '../../protocol';
import {
  SCENE_HUES,
  SCENE_NEUTRALS,
  SCENE_SESSION,
  SCENE_SKIP_THRESHOLD_MS,
  sessionMomentMs,
  type SceneSessionClass,
} from '../../sceneTokens';
import type { PresentationMode } from './presentationMode';

/** One staged session moment, named as the §8 table names it. */
export type SessionMoment =
  | 'game-start'
  | 'mulligan'
  | 'hand-kept'
  | 'reconnect'
  | 'victory'
  | 'defeat'
  | 'draw'
  | 'return-to-lobby';

/** The moments that stage the *entry* into a presented scene. */
export type EntryMoment = Extract<SessionMoment, 'game-start' | 'reconnect'>;

/** The terminal verdict moments, phrased from the receiving seat. */
export type VerdictMoment = Extract<SessionMoment, 'victory' | 'defeat' | 'draw'>;

/** The token class each moment reads its budget from. */
const MOMENT_CLASS: Record<SessionMoment, SceneSessionClass> = {
  'game-start': 'gameStart',
  mulligan: 'mulligan',
  'hand-kept': 'handKept',
  reconnect: 'reconnect',
  victory: 'victory',
  defeat: 'defeat',
  // A draw is neither a loss nor a gain moment; it rides the quiet defeat-side
  // window rather than the celebratory one.
  draw: 'defeat',
  'return-to-lobby': 'returnToLobby',
};

/**
 * The accent each moment wears. Only the §2 semantic families appear: gold for
 * flow and the victory bloom, red for the loss moment, and plain text neutral
 * for an outcome that is neither.
 */
const MOMENT_ACCENT: Record<SessionMoment, string> = {
  'game-start': SCENE_HUES.gold.value,
  mulligan: SCENE_HUES.gold.value,
  'hand-kept': SCENE_HUES.gold.value,
  reconnect: SCENE_HUES.gold.value,
  victory: SCENE_HUES.gold.value,
  defeat: SCENE_HUES.red.value,
  draw: SCENE_NEUTRALS.text,
  'return-to-lobby': SCENE_HUES.gold.value,
};

/** The staged window for a moment at standard motion, ms (its §8 budget). */
export function momentBudgetMs(moment: SessionMoment): number {
  return SCENE_SESSION[MOMENT_CLASS[moment]].ms;
}

/** The moment's binding §8 cap, ms — what the budget test pins. */
export function momentCapMs(moment: SessionMoment): number {
  return SCENE_SESSION[MOMENT_CLASS[moment]].cap;
}

/**
 * The staged window with reduced motion wired in: 0 ms means "already at the
 * end state", so a caller can start a timer unconditionally and a reduced-motion
 * player simply never sees a staged frame.
 */
export function momentDurationMs(moment: SessionMoment, reducedMotion: boolean): number {
  return sessionMomentMs(MOMENT_CLASS[moment], reducedMotion);
}

/**
 * Whether a moment ends early on deliberate input. True exactly for the rows the
 * grammar marks *skippable* — the ones that may compose past the
 * {@link SCENE_SKIP_THRESHOLD_MS} skip threshold.
 */
export function isSkippable(moment: SessionMoment): boolean {
  return SCENE_SESSION[MOMENT_CLASS[moment]].skippable;
}

/** The §2 hue family a moment wears. */
export function momentAccent(moment: SessionMoment): string {
  return MOMENT_ACCENT[moment];
}

/**
 * Whether a staged window is long enough that §8 *demands* a user skip. A row
 * may be marked skippable below this (the mulligan's sweep+deal composes past
 * its own travel budget), but no row above it may be left unskippable.
 */
export function demandsSkip(durationMs: number): boolean {
  return durationMs > SCENE_SKIP_THRESHOLD_MS;
}

/**
 * Classify the verdict from the receiving seat, exactly as the verdict panel
 * phrases it: a draw has no winner (CR 104.4a), otherwise the receiver won iff
 * they are the named winner. A receiver-less view (a spectator, `you: ''`) is
 * never "you won" — it reads the neutral draw-side staging while the panel still
 * names the winner. The client decides nothing here; it formats a result the
 * server already decided.
 */
export function verdictMoment(result: GameResult, you: PlayerId): VerdictMoment {
  if (result.winner === undefined) return 'draw';
  if (you !== '' && result.winner === you) return 'victory';
  // A seated player who is not the winner lost; a spectator has no stake, so the
  // moment stays neutral rather than staging someone else's defeat as theirs.
  return you === '' ? 'draw' : 'defeat';
}

/**
 * The entry moment for a presented view transition, or `null` when the
 * transition stages nothing.
 *
 * - `initial` — the first complete view this mount presents: the scene assembles
 *   from nothing, which is exactly the §8 **game start** choreography. A hard
 *   reload mid-game lands here too and gets the same assembly; the scene really
 *   is being built from nothing, and it stays inside the same ≤ 800 ms budget.
 * - `rebuild` — an in-session transport discontinuity (issue #493): the latest
 *   view renders complete and gets the single **reconnect** "you are here" cue.
 * - `reconcile` / `fast-forward` — ordinary play; no session moment.
 */
export function entryMoment(mode: PresentationMode): EntryMoment | null {
  if (mode === 'initial') return 'game-start';
  if (mode === 'rebuild') return 'reconnect';
  return null;
}
