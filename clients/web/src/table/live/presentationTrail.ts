/**
 * The **presentation trail** (issue #594) — the pure scheduler that turns the
 * server's ordered window of {@link PresentationMoment}s into one caption at a
 * time, at a pace a person can read.
 *
 * ADR 0020's settle loop applies many actions before it broadcasts, and the
 * per-seat channel is latest-value, so a receiver is handed a *final* board where
 * the game passed through a sequence of causal states. `GameView.presentation`
 * carries that sequence; this module decides which one is on screen right now and
 * when the next one takes its place.
 *
 * ## What this module is not allowed to do
 *
 * - **It never delays a view.** `store.view` is applied the instant it arrives;
 *   the trail paces a *caption* over a board that is already authoritative and
 *   already interactive (AGENTS.md hard rule, and `presentation-budgets.md`
 *   §Performance: hit targets exist at their final rects immediately). Nothing
 *   here returns a view, gates input, or holds a prompt.
 * - **It never reconstructs rules.** Every moment is a fact the server stated. The
 *   trail reorders nothing, invents nothing, fills no gap, and interprets no kind
 *   into an action: a receiver's id stream legitimately skips values (the window is
 *   bounded, and another seat's `phases_skipped` is filtered out of this one), so a
 *   missing id is not a lost message to wait for.
 * - **It holds no clock.** Every function is a total function of `(trail, input)`
 *   with `now` as a parameter — no timers, no `Date.now`, no React, no DOM, which
 *   is what makes the pacing testable without fake timers.
 *   `usePresentationTrail` owns the one `window.setTimeout` that calls
 *   {@link advance} at the {@link TrailAdvance.wakeAt} this module reports.
 *
 * ## Dwell is not animation
 *
 * The budgets in {@link PRESENTATION_DWELL} are **dwell**: how long a caption
 * stays legible. Reduced motion does not shorten them, for the same reason
 * {@link ACTIVITY.dwellMs} is unchanged under it — there is no motion in a
 * caption to remove, and a reader who asked for less movement did not ask to read
 * faster. It removes travel and tween only ({@link StagedMoment.travel}); the
 * staged *sequence* is identical with it on or off.
 */
import type { MomentKind, MomentKindTag, PresentationMoment } from '../../protocol';
import { SCENE_MOTION } from '../../sceneTokens';

/**
 * The dwell budgets, in ms — how long each kind of moment holds the caption
 * surface (issue #594's pacing table).
 *
 * These are **tuning ranges, not protocol constants**: nothing on the wire states
 * them, no server behaviour depends on them, and changing one changes only how
 * long a sentence sits on screen. They are frozen in one object beside the
 * derivation that reads them (the {@link ACTIVITY} idiom) so a reader can see the
 * whole pace at once rather than hunting literals through a scheduler.
 *
 * Each number is argued against `docs/design/presentation-budgets.md` §Animation,
 * whose motion classes bound the travel a dwell has to outlast — a caption that
 * retires before its own zone travel lands would describe an animation the reader
 * is still watching.
 */
export const PRESENTATION_DWELL = {
  /**
   * A spell going on the stack: 380 ms. Inside the `zoneTravel` window
   * (250–400 ms) and above its 320 ms default, so the card has arrived on the
   * stack before the caption that announced it leaves.
   */
  cast: 380,
  /**
   * A spell or ability leaving the stack — resolved, countered, or fizzled:
   * 420 ms. The longest of the three chain beats because it is the one carrying a
   * *distinction* a board diff cannot make (countered is not fizzled is not
   * resolved), and it sits well inside the `resolution` class cap of 600 ms.
   */
  resolution: 420,
  /**
   * A death or any other zone move: 340 ms. The shortest chain beat — by the time
   * it shows, the reader has already been told what caused it — and still above
   * the 320 ms `zoneTravel` default it accompanies.
   */
  zone: 340,
  /**
   * A turn boundary: 500 ms, the full `turnFlow` cap. The most expensive dwell in
   * the table, deliberately: issue #455 records the playtest failure verbatim —
   * *"the player believes they're still in turn 1; the game is at turn 2"* — and a
   * turn change is the single beat that costs the most to miss.
   */
  turnChange: SCENE_MOTION.turnFlow.cap,
  /**
   * One `phases_skipped` **group**: 320 ms for the whole path, however many steps
   * it names. A settle can pass a seat through a dozen priority windows; charging
   * a dwell per step would spend the entire window telling a player about the
   * nothing that happened to them. See {@link dwellMsFor}.
   */
  skipped: 320,
  /**
   * Everything else — damage, life, declarations, draws, phase changes, an
   * elimination, an unclassified kind: 260 ms. Long enough to register as its own
   * beat, short enough that a busy settle still reaches the moments worth watching.
   */
  other: 260,
  /**
   * The dwell an aggregated entry (`count > 1`) is cut to under compression:
   * 200 ms. A repeat has already said what it has to say; the count says the rest.
   */
  repeat: 200,
  /**
   * The compression floor: 160 ms, above the `micro` class cap of 150 ms so that
   * three floored chain beats still read as three beats and not one flash. Nothing
   * is ever cut below this and nothing is ever cut to zero — {@link compress}
   * hurries the trail, it does not skip it.
   */
  floor: 160,
} as const;

/**
 * The queued backlog, in ms, above which {@link compress} starts cutting: 1800.
 *
 * Roughly two seconds is the point at which a caption stops describing what the
 * board is doing and starts narrating history — and, worse, the point at which a
 * decision timer could begin running while the player is still watching cosmetic
 * playback. The hook additionally fast-forwards outright when a real decision
 * arrives; this is the softer of the two responses.
 */
export const PRESENTATION_BACKLOG_MS = 1800;

/**
 * The cosmetic backlog a *decision* tolerates, in ms: 900 — half the ordinary cap.
 *
 * {@link accelerate}'s budget. Five floored beats, enough to carry a cast, its
 * resolution and the death that followed with room to spare, and short enough that
 * an authoritative `action_deadline` cannot tick meaningfully behind captions
 * describing the frame before it. Above this the *oldest* beats are dropped, never
 * the newest — the tail is what explains the decision being asked.
 */
export const PRESENTATION_DECISION_MS = 900;

/**
 * The kinds {@link compress} may never drop: the causal chain (cast → resolution →
 * death/zone move) and the turn boundary.
 *
 * The chain is the entire reason this contract exists. Diffing two boards says a
 * creature is gone; only the ordered chain says whether it was countered, killed,
 * sacrificed, or exiled and returned. Collapsing it into one instant to catch up
 * would throw away exactly the information that could not be recovered any other
 * way, so compression takes its cuts from captions instead.
 */
const PROTECTED_KINDS: readonly MomentKindTag[] = [
  'cast',
  'resolved',
  'countered',
  'fizzled',
  'died',
  'zone_move',
  'turn_change',
];

/**
 * The kinds compression drops or merges first. A `phase_change` is a caption for a
 * position the view's own {@link GameView.phase} already states, and a `drew` is a
 * count the reader can see in a hand — neither leaves a hole in the record when it
 * goes.
 */
const LOW_VALUE_KINDS: readonly MomentKindTag[] = ['phase_change', 'drew'];

/**
 * The kinds that carry travel: an object physically moves across the scene for
 * them. Only these are affected by reduced motion, and only in their travel — the
 * caption, its order, and its dwell are identical either way.
 */
const TRAVEL_KINDS: readonly MomentKindTag[] = ['cast', 'died', 'zone_move'];

/** One moment as the trail holds it: the caption, its dwell, and its motion. */
export interface StagedMoment {
  /**
   * The moment to render. For an aggregated run this is the **first** moment of
   * the run with its {@link PresentationMoment.count} raised, so the entry keeps
   * the place — and the id, and therefore the React key — the run started at.
   */
  moment: PresentationMoment;
  /** How long this entry holds the surface, in ms. Always > 0. */
  dwellMs: number;
  /**
   * Whether the surface may animate travel for this moment. False under reduced
   * motion and false for kinds that never travel; the caption shows regardless.
   */
  travel: boolean;
}

/**
 * The scheduler's whole state. Immutable: every function returns a new trail
 * rather than mutating this one, so a React store can hold it directly and
 * identity comparison is a correct "did anything change" test.
 *
 * Losing a trail loses no game information — a fresh one simply starts pacing from
 * the next window, which is the right behaviour after a reconnect.
 */
export interface PresentationTrail {
  /** The moment on screen now, or `null` when the surface is quiet. */
  readonly staged: StagedMoment | null;
  /** The clock time {@link staged} retires at. Meaningless when `staged` is null. */
  readonly stagedUntil: number;
  /** The moments waiting their turn, oldest first. */
  readonly queue: readonly StagedMoment[];
  /**
   * The highest moment id ever admitted. The de-duplication watermark: an
   * overlapping window costs nothing, and a duplicate delivery of a window already
   * seen is a no-op. Ids are opaque ordering handles — nothing is derived from the
   * arithmetic between them, and a gap is never treated as a lost message.
   */
  readonly watermark: number;
  /** Whether the player asked for reduced motion (travel only; never the dwell). */
  readonly reducedMotion: boolean;
}

/** What one {@link advance} produced. */
export interface TrailAdvance {
  /** The trail after the tick. Identical (by reference) when nothing changed. */
  trail: PresentationTrail;
  /** The moment now on screen, or `null` when the surface is quiet. */
  staged: StagedMoment | null;
  /**
   * The clock time the caller should call {@link advance} again at, or `null` when
   * there is nothing left to stage and no timer is needed.
   */
  wakeAt: number | null;
}

/** An empty trail. `reducedMotion` may change later through {@link withReducedMotion}. */
export function createPresentationTrail(
  options: { reducedMotion?: boolean } = {},
): PresentationTrail {
  return {
    staged: null,
    stagedUntil: 0,
    queue: [],
    // Below every possible server id, so the first window is admitted whole
    // however late in a room's life this client joined.
    watermark: -1,
    reducedMotion: options.reducedMotion ?? false,
  };
}

/**
 * The same trail with a new motion preference (issue #505 lets a player change it
 * mid-match). Only the travel flag of *unplayed* entries moves: what is already on
 * screen keeps the treatment it started with rather than switching under the eye.
 */
export function withReducedMotion(
  trail: PresentationTrail,
  reducedMotion: boolean,
): PresentationTrail {
  if (trail.reducedMotion === reducedMotion) return trail;
  return {
    ...trail,
    reducedMotion,
    queue: trail.queue.map((entry) => ({ ...entry, travel: travels(entry.moment, reducedMotion) })),
  };
}

/** Whether the trail has nothing staged and nothing waiting. */
export function isTrailIdle(trail: PresentationTrail): boolean {
  return trail.staged === null && trail.queue.length === 0;
}

/** The tag of a moment's kind, or `null` when the server named a kind this build
 * does not know ({@link PresentationMoment.kindUnknown}). */
function tagOf(moment: PresentationMoment): MomentKindTag | null {
  return moment.kind?.kind ?? null;
}

/**
 * The dwell one moment earns, in ms.
 *
 * A `phases_skipped` is the case worth stating: it gets
 * {@link PRESENTATION_DWELL.skipped} for the **whole group**, independent of how
 * many steps it names. The server already folds a seat's entire auto-passed path
 * into one moment; charging per step here would undo that on the client.
 */
export function dwellMsFor(moment: PresentationMoment): number {
  switch (tagOf(moment)) {
    case 'cast':
      return PRESENTATION_DWELL.cast;
    case 'resolved':
    case 'countered':
    case 'fizzled':
      return PRESENTATION_DWELL.resolution;
    case 'died':
    case 'zone_move':
      return PRESENTATION_DWELL.zone;
    case 'turn_change':
      return PRESENTATION_DWELL.turnChange;
    case 'phases_skipped':
      return PRESENTATION_DWELL.skipped;
    default:
      return PRESENTATION_DWELL.other;
  }
}

/** Whether a moment is part of the causal chain compression may never drop. */
export function isProtectedMoment(moment: PresentationMoment): boolean {
  const tag = tagOf(moment);
  return tag !== null && PROTECTED_KINDS.includes(tag);
}

/** Whether this moment animates travel, given the motion preference. */
function travels(moment: PresentationMoment, reducedMotion: boolean): boolean {
  if (reducedMotion) return false;
  const tag = tagOf(moment);
  return tag !== null && TRAVEL_KINDS.includes(tag);
}

/** Structural equality, used only to recognise a repeat of the same kind. */
function sameValue(a: unknown, b: unknown): boolean {
  if (a === b) return true;
  if (Array.isArray(a) || Array.isArray(b)) {
    if (!Array.isArray(a) || !Array.isArray(b) || a.length !== b.length) return false;
    return a.every((item, index) => sameValue(item, b[index]));
  }
  if (typeof a !== 'object' || typeof b !== 'object' || a === null || b === null) return false;
  const left = a as Record<string, unknown>;
  const right = b as Record<string, unknown>;
  const keys = Object.keys(left);
  if (keys.length !== Object.keys(right).length) return false;
  return keys.every((key) => key in right && sameValue(left[key], right[key]));
}

/** Whether two kinds are the same event down to their payload. */
function sameKind(a: MomentKind | undefined, b: MomentKind | undefined): boolean {
  // An unclassified moment never aggregates: this build cannot say what it was, so
  // it cannot say two of them were the same thing.
  if (a === undefined || b === undefined) return false;
  return a.kind === b.kind && sameValue(a, b);
}

/** Wrap a moment as a queue entry. */
function entryOf(moment: PresentationMoment, reducedMotion: boolean): StagedMoment {
  return {
    moment,
    dwellMs: dwellMsFor(moment),
    travel: travels(moment, reducedMotion),
  };
}

/**
 * Fold `next` into `tail` when the two belong on one caption, else `null`.
 *
 * Two foldings, and no others:
 *
 * - **A repeat.** Identical kinds — six triggers of one ability, four instances of
 *   the same damage — become one entry with the count raised, mirroring the
 *   aggregation the server already does across a batch boundary it cannot see.
 * - **A phase trail.** Consecutive `phase_change`s become one entry showing the
 *   *latest* phase, because a run of steps crossed in one settle is one readable
 *   update ("now in declare attackers"), not five captions the reader watches
 *   scroll past.
 *
 * Nothing else folds. In particular the chain never does: `cast`, `resolved` and
 * `died` are three different kinds, so no aggregation path can turn three beats
 * into one instant.
 */
function fold(tail: StagedMoment, next: StagedMoment): StagedMoment | null {
  const count = tail.moment.count + Math.max(1, next.moment.count);
  if (tagOf(tail.moment) === 'phase_change' && tagOf(next.moment) === 'phase_change') {
    return { ...tail, moment: { ...tail.moment, kind: next.moment.kind, count } };
  }
  if (!sameKind(tail.moment.kind, next.moment.kind)) return null;
  return { ...tail, moment: { ...tail.moment, count } };
}

/**
 * Admit a freshly received window.
 *
 * Moments at or below the watermark are dropped (an overlapping window, a
 * duplicate delivery, a redelivery after a reconnect), the rest are appended **in
 * the order given** — never sorted, never re-ordered, never gap-filled — folding
 * into the tail where {@link fold} says they belong.
 *
 * Only the *queue* tail folds, never the staged entry: a count must not change
 * under a caption the reader is already reading.
 *
 * This appends without bound on purpose; the caller decides how to catch up.
 * {@link compress} is the ordinary answer and {@link fastForward} the abrupt one.
 */
export function enqueue(
  trail: PresentationTrail,
  moments: readonly PresentationMoment[],
): PresentationTrail {
  let watermark = trail.watermark;
  let queue: StagedMoment[] | null = null;
  for (const moment of moments) {
    if (!Number.isFinite(moment.id) || moment.id <= watermark) continue;
    watermark = moment.id;
    queue ??= [...trail.queue];
    const tail = queue[queue.length - 1];
    const entry = entryOf(moment, trail.reducedMotion);
    const folded = tail === undefined ? null : fold(tail, entry);
    if (folded === null) queue.push(entry);
    else queue[queue.length - 1] = folded;
  }
  if (queue === null) return trail;
  return { ...trail, queue, watermark };
}

/**
 * Tick the clock: retire the staged moment when its dwell has elapsed and stage
 * the next one.
 *
 * **Idempotent for a given `now`** — calling it twice at the same clock reading
 * yields the same staged moment and the same wake time, so a component may call it
 * from an effect, a timer, and a render without double-advancing.
 *
 * A caller that wakes *late* (a backgrounded tab, a busy frame) does not lose the
 * moments it slept through: the newly staged entry gets its full dwell from `now`
 * rather than from the deadline it missed. Running late is the trail's problem to
 * solve by compressing or fast-forwarding, never by silently skipping a beat.
 */
export function advance(trail: PresentationTrail, now: number): TrailAdvance {
  if (trail.staged !== null && now < trail.stagedUntil) {
    return { trail, staged: trail.staged, wakeAt: trail.stagedUntil };
  }
  const [next, ...rest] = trail.queue;
  if (next === undefined) {
    if (trail.staged === null) return { trail, staged: null, wakeAt: null };
    // The last moment's dwell expired with nothing behind it: the caption retires
    // and the surface goes quiet, exactly as the activity ticker's does.
    return { trail: { ...trail, staged: null, stagedUntil: 0 }, staged: null, wakeAt: null };
  }
  const stagedUntil = now + next.dwellMs;
  return {
    trail: { ...trail, staged: next, stagedUntil, queue: rest },
    staged: next,
    wakeAt: stagedUntil,
  };
}

/** The queued backlog in ms — what the reader still has to sit through. */
export function backlogMs(trail: PresentationTrail): number {
  return trail.queue.reduce((total, entry) => total + entry.dwellMs, 0);
}

/** Whether the backlog has grown past {@link PRESENTATION_BACKLOG_MS}. */
export function needsCompression(trail: PresentationTrail): boolean {
  return backlogMs(trail) > PRESENTATION_BACKLOG_MS;
}

/** Re-fold neighbours that became adjacent when something between them was cut. */
function mergeRuns(entries: StagedMoment[]): StagedMoment[] {
  const merged: StagedMoment[] = [];
  for (const entry of entries) {
    const tail = merged[merged.length - 1];
    const folded = tail === undefined ? null : fold(tail, entry);
    if (folded === null) merged.push(entry);
    else merged[merged.length - 1] = folded;
  }
  return merged;
}

/** The total dwell of a list — the same measure {@link backlogMs} reports. */
function totalMs(entries: readonly StagedMoment[]): number {
  return entries.reduce((total, entry) => total + entry.dwellMs, 0);
}

/** Drop matching entries, oldest first, only while the list is over budget. */
function dropWhileOver(
  entries: StagedMoment[],
  matches: (entry: StagedMoment) => boolean,
): StagedMoment[] {
  let remaining = totalMs(entries);
  const kept: StagedMoment[] = [];
  for (const entry of entries) {
    if (remaining > PRESENTATION_BACKLOG_MS && matches(entry)) {
      remaining -= entry.dwellMs;
      continue;
    }
    kept.push(entry);
  }
  return kept;
}

/** Whether an entry is one of the {@link LOW_VALUE_KINDS} compression cuts first. */
function matchesLowValue(entry: StagedMoment): boolean {
  const tag = tagOf(entry.moment);
  return tag !== null && LOW_VALUE_KINDS.includes(tag);
}

/** Cut matching dwells to `target`, oldest first, only while over budget. */
function shortenWhileOver(
  entries: StagedMoment[],
  matches: (entry: StagedMoment) => boolean,
  target: number,
): StagedMoment[] {
  let remaining = totalMs(entries);
  return entries.map((entry) => {
    if (remaining <= PRESENTATION_BACKLOG_MS || !matches(entry) || entry.dwellMs <= target) {
      return entry;
    }
    remaining -= entry.dwellMs - target;
    return { ...entry, dwellMs: target };
  });
}

/**
 * Catch the backlog up when it has grown past {@link PRESENTATION_BACKLOG_MS}.
 * A no-op (returning the same trail) below that, so a caller may run it after
 * every window without thinking about it.
 *
 * Five passes, each stopping the moment the backlog is back under the cap, so the
 * trail is cut as little as the situation demands:
 *
 * 1. merge neighbouring repeats and phase runs;
 * 2. drop `phase_change` captions — the view's own phase is current regardless;
 * 3. merge again, since dropping made new neighbours adjacent;
 * 4. drop `drew` captions;
 * 5. cut repeats to {@link PRESENTATION_DWELL.repeat}, then everything left to
 *    {@link PRESENTATION_DWELL.floor}.
 *
 * **The chain is never dropped.** A backlog of nothing but casts, resolutions,
 * deaths, zone moves and turn changes bottoms out at the floor and simply plays
 * fast — possibly still over the cap. That is the deliberate trade: a trail that
 * runs long is a pacing problem, while a trail that swallows a beat is a
 * *correctness* problem, because the order those states happened in is not
 * recoverable from anywhere else. The hook's {@link fastForward} is the escape
 * hatch for when the player needs to be here now.
 */
export function compress(trail: PresentationTrail): PresentationTrail {
  if (!needsCompression(trail)) return trail;
  let queue = mergeRuns([...trail.queue]);
  queue = dropWhileOver(queue, (entry) => tagOf(entry.moment) === 'phase_change');
  queue = mergeRuns(queue);
  // Pass 2 already took the phase captions; what this reaches is the draws.
  queue = dropWhileOver(queue, matchesLowValue);
  queue = shortenWhileOver(queue, (entry) => entry.moment.count > 1, PRESENTATION_DWELL.repeat);
  queue = shortenWhileOver(queue, () => true, PRESENTATION_DWELL.floor);
  return { ...trail, queue };
}

/**
 * Abandon the backlog and park at the newest moment already seen — the reconnect
 * and the restored background tab.
 *
 * The watermark stays where it is, which is the whole point: everything dropped
 * here is *already reflected in the view* the client is rendering, so a redelivery
 * of the same window (a reconnect resend, an overlapping bounded window) is a
 * no-op rather than a replay of history the player has stopped caring about, while
 * a later batch still plays normally.
 *
 * Reserved for a **discontinuity** — a session that no longer exists, or a tab that
 * slept through more than a whole backlog. Both mean the queued moments describe a
 * past the player has no stake in seeing narrated. The arrival of a *decision* is
 * not one of those: see {@link accelerate}.
 */
export function fastForward(trail: PresentationTrail): PresentationTrail {
  if (isTrailIdle(trail)) return trail;
  return { ...trail, staged: null, stagedUntil: 0, queue: [] };
}

/**
 * Hurry the backlog out of a decision's way **without deleting the causal chain** —
 * what issue #594 calls "accelerate *or* fast-forward", taking the first.
 *
 * Dropping the queue outright when a decision arrives fails the contract's headline
 * acceptance criterion. The server settles a removal to rest with the *caster*
 * holding priority, so the window carrying cast → resolved → died reaches the
 * helpless seat on a broadcast with no actions on it, and that seat's own decision
 * arrives on the *next* one — often milliseconds later against an AI or a fast
 * opponent. Discarding the backlog there leaves the player with exactly the board
 * diff this whole contract exists to replace: the creature is gone, and nothing ever
 * said what killed it. Those ids are at or below the watermark, so nothing replays
 * them.
 *
 * So a decision cuts the backlog down instead of throwing it away:
 * - everything unprotected goes at once — a `phase_change` names a position the
 *   view's own `phase` already states, and a `drew` is not worth a decision's clock;
 * - every surviving beat drops to {@link PRESENTATION_DWELL.floor};
 * - and if even the floored chain would outlast {@link PRESENTATION_DECISION_MS},
 *   the *oldest* beats go first, because the newest are the ones explaining the
 *   decision now on the table.
 *
 * What is already on screen is left alone. It is mid-read, it is bounded by one
 * dwell, and cutting a caption off under the eye is the one thing this module never
 * does. Nothing here gates input either way: the board and its `valid_actions` went
 * live the instant the view arrived, so this bounds how long a *timer* can tick
 * behind a caption, not how long a player waits to act.
 */
export function accelerate(trail: PresentationTrail): PresentationTrail {
  if (isTrailIdle(trail)) return trail;
  let queue = mergeRuns(trail.queue.filter((entry) => isProtectedMoment(entry.moment)));
  queue = queue.map((entry) =>
    entry.dwellMs <= PRESENTATION_DWELL.floor
      ? entry
      : { ...entry, dwellMs: PRESENTATION_DWELL.floor },
  );
  while (queue.length > 0 && totalMs(queue) > PRESENTATION_DECISION_MS) queue = queue.slice(1);
  return { ...trail, queue };
}
