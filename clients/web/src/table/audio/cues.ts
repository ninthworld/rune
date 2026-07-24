/**
 * The pure presentation-intent → sound/haptic cue mapping (issue #507).
 *
 * The visual channel already derives every event this taxonomy names
 * (`../live/gameViewPresentation`), so the hook layer **subscribes to the same
 * stream** rather than re-reading the view: motion intents supply the scene
 * events, and the transition's log entries supply the two session moments the
 * scene deliberately leaves to their own chrome (`game_over`,
 * `player_eliminated`).
 *
 * Two properties are load-bearing for the acceptance criteria and are enforced
 * here rather than in a sink:
 *
 * - **Batch collapse.** Cues are grouped by `(category, batch window)` and
 *   emitted once per group, mirroring the visual stagger budget
 *   (`SCENE_BATCH.windowMs`). A thirty-token swarm stages as thirty visual
 *   arrivals inside one window and therefore makes **one** sound.
 * - **Reduced motion is not silence.** Motion and audio are independent
 *   channels (visual-system §9); reduced motion zeroes the visual stagger, which
 *   only tightens every cue onto the window's leading edge. It never removes a
 *   cue. Silencing is the settings surface's job, not accessibility's.
 *
 * This module is pure and does no I/O.
 */
import type { GameLogEntry, PlayerId } from '../../protocol';
import { SCENE_BATCH } from '../../sceneTokens';
import type {
  GameViewMotionCategory,
  GameViewMotionIntent,
  GameViewPresentation,
} from '../live/gameViewPresentation';
import type { AudioCue, AudioCueCategory } from './types';

/**
 * Motion class → taxonomy category. The visual grammar is finer-grained than
 * the ten-category sound taxonomy, so several classes share a cue; that is the
 * taxonomy's intent, not a loss.
 *
 * `focus` is deliberately absent: the staging cue is a camera move, not a game
 * event, and giving the client's own framing a sound would make audio narrate
 * something the server never said.
 */
export const MOTION_CUE_CATEGORY: Readonly<
  Partial<Record<GameViewMotionCategory, AudioCueCategory>>
> = Object.freeze({
  draw: 'draw',
  play: 'play',
  'zone-travel': 'play',
  'battlefield-entry': 'play',
  'token-batch': 'play',
  tap: 'tap',
  untap: 'tap',
  cast: 'cast',
  resolve: 'resolve',
  counter: 'resolve',
  fizzle: 'resolve',
  damage: 'impact',
  heal: 'impact',
  attack: 'impact',
  block: 'impact',
  'counter-change': 'impact',
  death: 'destroy',
  priority: 'priority',
  phase: 'phase',
  turn: 'phase',
});

/** Effect-layer anchor prefixes that name a seat rather than an entity. */
const SEAT_ANCHOR_PREFIXES = ['seat:', 'pile:', 'hand:'];

/** The seat an anchor reference names, when it names one. */
function anchorSeat(anchor: string | undefined): PlayerId | undefined {
  if (anchor === undefined) return undefined;
  const prefix = SEAT_ANCHOR_PREFIXES.find((candidate) => anchor.startsWith(candidate));
  return prefix === undefined ? undefined : anchor.slice(prefix.length);
}

/** The seat a motion intent is credited to: its destination, else its source. */
function motionSeat(motion: GameViewMotionIntent): PlayerId | undefined {
  return anchorSeat(motion.to) ?? anchorSeat(motion.from);
}

/**
 * The batch window a delay falls in. Every stagger the adapter assigns is
 * capped at `SCENE_BATCH.windowMs`, so in practice one transition yields one
 * window per category — which is exactly the "one sound per batch window"
 * guarantee. The floor is kept general so a future longer window still
 * collapses correctly.
 */
function batchWindow(delayMs: number): number {
  return Math.floor(Math.max(0, delayMs) / SCENE_BATCH.windowMs);
}

/** A cue under construction, before the group is closed. */
interface Accumulator extends AudioCue {
  window: number;
}

/**
 * Fold one contribution into the accumulator for its `(category, window)` key.
 * The first contribution fixes the seat and the leading-edge delay; later ones
 * only raise the count and the reported magnitude.
 */
function accumulate(
  groups: Map<string, Accumulator>,
  order: string[],
  category: AudioCueCategory,
  delayMs: number,
  magnitude: number | undefined,
  seat: PlayerId | undefined,
): void {
  const window = batchWindow(delayMs);
  const key = `${category}:${window}`;
  const size = magnitude === undefined ? undefined : Math.abs(magnitude);
  const existing = groups.get(key);
  if (existing === undefined) {
    groups.set(key, {
      category,
      window,
      delayMs,
      count: 1,
      ...(size === undefined ? {} : { magnitude: size }),
      ...(seat === undefined ? {} : { seat }),
    });
    order.push(key);
    return;
  }
  existing.count += 1;
  existing.delayMs = Math.min(existing.delayMs, delayMs);
  if (size !== undefined) existing.magnitude = Math.max(existing.magnitude ?? 0, size);
  if (existing.seat === undefined && seat !== undefined) existing.seat = seat;
}

/**
 * The taxonomy cues carried by log entries alone. `game_over` is the only
 * source of `victory` — the scene leaves the verdict to its own chrome, so no
 * motion intent ever carries it — and an elimination reuses the destruction
 * grammar the visual channel already gives it.
 */
function accumulateEvents(
  entries: readonly GameLogEntry[],
  groups: Map<string, Accumulator>,
  order: string[],
): void {
  for (const { event } of entries) {
    if (event.type === 'game_over') {
      accumulate(groups, order, 'victory', 0, undefined, event.result.winner);
    } else if (event.type === 'player_eliminated') {
      accumulate(groups, order, 'destroy', 0, undefined, event.player);
    }
  }
}

/**
 * Derive the collapsed sound/haptic cues for one authoritative view transition.
 *
 * Pure, cheap, and total: an empty presentation yields no cues, and no input
 * can make it throw. The result is deterministic — session moments first (the
 * order the adapter reads the log in), then motion classes in intent order.
 */
export function deriveAudioCues(presentation: GameViewPresentation): AudioCue[] {
  const groups = new Map<string, Accumulator>();
  const order: string[] = [];
  accumulateEvents(presentation.events ?? [], groups, order);
  for (const motion of presentation.motions) {
    const category = MOTION_CUE_CATEGORY[motion.category];
    if (category === undefined) continue;
    accumulate(groups, order, category, motion.delayMs, motion.magnitude, motionSeat(motion));
  }
  return order.map((key) => {
    // `window` is grouping bookkeeping, not part of the cue contract.
    const accumulator = groups.get(key)!;
    const cue: AudioCue = {
      category: accumulator.category,
      delayMs: accumulator.delayMs,
      count: accumulator.count,
      ...(accumulator.magnitude === undefined ? {} : { magnitude: accumulator.magnitude }),
      ...(accumulator.seat === undefined ? {} : { seat: accumulator.seat }),
    };
    return cue;
  });
}
