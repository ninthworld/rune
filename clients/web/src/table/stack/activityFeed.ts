/**
 * The **activity surface**'s pure derivation — what collapses to a badge, what
 * surfaces automatically, and what the full history holds (issue #534, under
 * [ADR 0032](../../../../../docs/decisions/0032-contextual-shell-anatomy.md)).
 *
 * ADR 0032 removed the permanent activity column. The log's job did not change —
 * `ui-requirements.md` §Comprehension still wants a newcomer to be able to follow
 * what happened — so the column becomes three things with different costs:
 *
 * 1. a **badge** (one 44 px edge control) that is the explicit door to the full
 *    history and states how much is unread;
 * 2. an **auto-surfaced ticker** of the newest *meaningful* lines, which appears
 *    on its own when something worth reading happens and dwells out again; and
 * 3. the **full history**, which is the shipped {@link GameLog} panel, opened as
 *    a dismissible surface rather than parked on the battlefield.
 *
 * Only (1) and (2) are derived here. (3) is composed, not reimplemented.
 *
 * ## Composition semantics are carried, not rewritten
 *
 * Every line's words come from `table/logComposition.ts` — {@link describeEvent}
 * for the sentence and its clickable references, {@link groupEntries} for the
 * step-run folding. That module is the one place the client turns engine facts
 * into prose and it stays the one place: this module decides *which* entries
 * surface and *how many*, never *what they say*.
 *
 * ## What "meaningful" means, and why it is not a rules judgment
 *
 * A `step_changed` entry is the repetitive turn/phase advance that
 * `groupEntries` already folds behind one summary because it drowns the signal.
 * Everything else in the event set is a thing that happened. So "meaningful" is
 * exactly "not a step change" — a test on the event *type the server sent*, in
 * agreement with a judgement the shipped log already makes. Nothing is inferred
 * about a card, a rule, or a state change (AGENTS.md hard rule), and an unknown
 * future event type is treated as meaningful rather than silently swallowed:
 * degrading toward *showing* an unfamiliar event is the safe direction for a
 * comprehension surface.
 *
 * ## Ephemerality
 *
 * {@link ActivityModel} is a pure function of the view plus two ephemeral
 * presentation inputs (the unread marker from `useUnreadLog`, and how far the
 * reader has let the ticker dwell). Losing either loses no game information and a
 * fresh mount rebuilds a complete surface, so the reconnect invariant holds.
 *
 * Consumed by {@link ActivitySurface} (`ActivitySurface.tsx`), its only
 * production caller.
 */
import type { GameLogEntry, GameLogEvent, GameView } from '../../protocol';
import { describeEvent, type LogSegment } from '../logComposition';

/** The subset of a `GameView` the activity surface reads. */
export type ActivityView = Pick<GameView, 'log' | 'player_names'>;

/** One line the ticker draws: a log entry already composed into segments. */
export interface ActivityLine {
  /** The entry's `sequence` — the id the unread marker and React keys use. */
  sequence: number;
  /** The composed sentence, from {@link describeEvent}. Never authored here. */
  segments: LogSegment[];
  /** Whether the reader has not yet seen this entry (issue #340's marker). */
  unseen: boolean;
}

/** Everything the activity component needs for one view. */
export interface ActivityModel {
  /**
   * Whether the surface renders at all. **False when the log window is empty** —
   * no badge, no ticker, no reserved space, matching the stage's empty rule.
   */
  present: boolean;
  /** Entries in the carried window (bounded by the server, ADR 0021). */
  total: number;
  /** How many of them are unread. */
  unread: number;
  /** The badge's drawn text: the unread count, or the history glyph when caught up. */
  badgeText: string;
  /** The badge's accessible name — always a sentence, never a bare glyph (§3.1). */
  badgeLabel: string;
  /**
   * The lines surfacing right now, **newest first**. Empty once the reader has
   * let them dwell out, which is what keeps the surface from becoming the column
   * ADR 0032 removed.
   */
  surfaced: ActivityLine[];
}

/** Activity-surface budgets. */
export const ACTIVITY = {
  /** How many lines the auto-surfaced ticker shows at once. */
  surfaceMax: 3,
  /**
   * How long a surfaced line dwells before the ticker retires it, in ms.
   *
   * Sized against `presentation-budgets.md` §Animation — long enough to read
   * three short lines, and deliberately far above the ≤ 500 ms "turn / phase /
   * priority transition" row, because this is a *reading* surface and not a
   * transition cue. It is a dwell, not an animation: nothing moves, and the
   * reduced-motion path uses the same number (there is no motion to remove).
   */
  dwellMs: 6000,
  /** The count above which the badge reads `9+` rather than growing. */
  badgeMax: 9,
} as const;

/**
 * Whether an event is worth surfacing on its own. See the module note: this is a
 * test on the server's event type, in agreement with `groupEntries`' folding.
 */
export function isMeaningful(event: GameLogEvent): boolean {
  return event.type !== 'step_changed';
}

/**
 * The activity model for one view.
 *
 * `sinceSequence` is the ticker's dwell watermark: entries at or below it have
 * already had their moment on screen. The component advances it on a timer, and a
 * fresh mount starts it at `-1` so the newest lines surface once — which is the
 * right behaviour after a reconnect, where the player has just arrived and has
 * not read anything.
 */
export function deriveActivity(
  view: ActivityView,
  options: {
    /** Unread count from `useUnreadLog` (issue #340). */
    unreadCount?: number;
    /** Whether an entry's sequence is unseen, from the same hook. */
    isUnseen?: (sequence: number) => boolean;
    /** The highest sequence the ticker has already retired. */
    sinceSequence?: number;
  } = {},
): ActivityModel {
  const entries: GameLogEntry[] = view.log ?? [];
  const unread = options.unreadCount ?? 0;
  if (entries.length === 0) {
    return {
      present: false,
      total: 0,
      unread: 0,
      badgeText: '',
      badgeLabel: '',
      surfaced: [],
    };
  }

  const since = options.sinceSequence ?? -1;
  const surfaced: ActivityLine[] = [];
  // Walk newest-first and stop at the dwell watermark: the ticker is a "what
  // just happened" surface, so the newest line reads first and older ones fall
  // off the bottom rather than pushing the newest out of view.
  for (let i = entries.length - 1; i >= 0 && surfaced.length < ACTIVITY.surfaceMax; i -= 1) {
    const entry = entries[i];
    if (entry.sequence <= since) break;
    if (!isMeaningful(entry.event)) continue;
    const segments = describeEvent(entry.event, view);
    // An event kind the composer does not know yields no words; showing an empty
    // line would be worse than showing nothing.
    if (segments.length === 0) continue;
    surfaced.push({
      sequence: entry.sequence,
      segments,
      unseen: options.isUnseen?.(entry.sequence) ?? false,
    });
  }

  const badgeText =
    unread > 0 ? (unread > ACTIVITY.badgeMax ? `${ACTIVITY.badgeMax}+` : String(unread)) : '≡';
  const badgeLabel =
    unread > 0
      ? `Activity — ${unread} new ${unread === 1 ? 'event' : 'events'}. Open the full history.`
      : `Activity — ${entries.length} ${entries.length === 1 ? 'event' : 'events'}. Open the full history.`;

  return { present: true, total: entries.length, unread, badgeText, badgeLabel, surfaced };
}

/**
 * The newest sequence in the window — the value the ticker's dwell timer sets as
 * its watermark when the dwell expires. Exported so the component never has to
 * reach into the entry array itself.
 */
export function newestSequence(view: ActivityView): number {
  const entries = view.log ?? [];
  return entries.length > 0 ? entries[entries.length - 1].sequence : -1;
}
