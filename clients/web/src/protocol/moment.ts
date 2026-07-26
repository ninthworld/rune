/**
 * Presentation moments: the bounded, ordered window of display-only pacing cues
 * that rides every view (issue #594).
 *
 * The TypeScript mirror of `crates/rune-protocol/src/moment.rs`. Every shape here is
 * wire-facing and snake_case; the one exception is
 * {@link PresentationMoment.kindUnknown}, which is normalization state and says so by
 * being camelCase (the same convention {@link StackItem.kindUnknown} follows).
 */

import type { EntityId, PlayerId } from './index.js';
import type { CardView } from './card.js';
import type { LogBlock, LogDamageTarget } from './log.js';
import type { GameOverReason, GameResult } from './result.js';
import type { AutoPassedStep, Phase } from './view.js';

/**
 * The maximum number of {@link PresentationMoment}s one view carries (issue #594).
 *
 * The window is **bounded on purpose**, and the bound is part of the contract rather
 * than a server implementation detail: a settle can apply dozens of actions between two
 * broadcasts, and a view that grew without limit would make the pathological case (a
 * long chain of triggers, an AI-only table racing ahead) the most expensive message the
 * protocol sends, at exactly the moment the receiver is least able to watch it.
 *
 * A client that is behind by more than this has already missed moments and must say so
 * by **catching up** — never by asking for the rest. There is no backfill request in
 * this protocol and there will not be one; a moment that fell off the window is gone,
 * and the board it described is already in the view.
 */
export const PRESENTATION_WINDOW = 32;

/**
 * Every {@link MomentZone} value, mirroring the `MomentZone` enum's snake_case serde
 * encoding in `crates/rune-protocol/src/moment.rs`. This is the single source of truth:
 * the {@link MomentZone} union is derived from it and {@link isMomentZone} validates a
 * wire value against it, so a zone added here can never drift out of the type.
 */
export const MOMENT_ZONES = [
  'battlefield',
  'graveyard',
  'exile',
  'hand',
  'library',
  'stack',
  'command',
] as const;

/**
 * A zone endpoint of a {@link MomentKind} `zone_move` (issue #594): the battlefield
 * (CR 403), a graveyard (CR 404), exile (CR 406), a hand (CR 402), a library (CR 401),
 * the stack (CR 405), or the command zone (CR 408).
 *
 * A closed, **presentational** vocabulary: it names where a client should animate from
 * and to, not the engine's zone model. Both endpoints are stated because the
 * destination alone cannot say what kind of movement this was — `graveyard →
 * battlefield` is a reanimation and `hand → battlefield` is a land drop — and neither
 * is recoverable from a board diff that shows only the arrival. `hand` and `library`
 * are endpoints only: a moment never retains a hidden *face* (see {@link MomentObject}).
 */
export type MomentZone = (typeof MOMENT_ZONES)[number];

/** Whether a wire value is a known {@link MomentZone}. */
export function isMomentZone(value: unknown): value is MomentZone {
  return typeof value === 'string' && (MOMENT_ZONES as readonly string[]).includes(value);
}

/**
 * Every {@link AutoPassReason} value, mirroring the enum's snake_case serde encoding.
 * The single source of truth for the union and for {@link isAutoPassReason}.
 */
export const AUTO_PASS_REASONS = ['no_response_available', 'forced_declaration'] as const;

/**
 * Why the room passed priority for a seat in a `phases_skipped` moment (issue #594,
 * ADR 0020):
 * - `no_response_available` — the seat had no meaningful action at those positions
 *   (CR 117.3 priority with nothing to do) and had not asked to stop there: the
 *   ordinary ADR 0020 pass.
 * - `forced_declaration` — the seat faced a declaration with no legal non-empty answer
 *   (issue #453): a `declare_attackers` with no eligible attacker, a `declare_blockers`
 *   with no eligible blocker. The room submitted the empty declaration rather than
 *   prompting for a choice that did not exist.
 *
 * Two reasons because they are two different sentences to a player. The judgment is the
 * **server's alone**: a client renders the stated reason and never derives it from the
 * board — working out "they had nothing to do" from the legal-action set would be
 * exactly the rules interpretation ADR 0002 puts on the server.
 */
export type AutoPassReason = (typeof AUTO_PASS_REASONS)[number];

/** Whether a wire value is a known {@link AutoPassReason}. */
export function isAutoPassReason(value: unknown): value is AutoPassReason {
  return typeof value === 'string' && (AUTO_PASS_REASONS as readonly string[]).includes(value);
}

/**
 * The **retained snapshot** of an object a moment refers to (issue #594).
 *
 * A moment is shown *after* the state it describes is gone: the creature that died is
 * not on the battlefield, the countered spell is not on the stack, and the view carrying
 * the moment is the one in which they are already absent. A reference by id alone would
 * therefore resolve to nothing at exactly the moment it is needed, which is why the name
 * — and, when the room had one, the face — travel with the moment instead of being
 * looked up in the current view.
 *
 * **Fixed at record time, never re-resolved.** The name is what the object was called
 * when the server recorded the moment, the same promise {@link LogEntity.name} makes. A
 * later copy effect, a name change, or a new object reusing the id does not
 * retroactively edit history, and a client must not "correct" a retained name against
 * the board.
 *
 * **Public faces only.** {@link card} is present only when the object was visible in a
 * public zone — battlefield, stack, graveyard, exile, command — when the server observed
 * it. A hand or library face is **never** retained, for any receiver, under any
 * aggregation: these moments cross seats (a spectator and every opponent read the same
 * public ones), so a retained private face would be an information leak with no way to
 * redact it after the fact. An absent `card` means "no public face was known", never
 * "hidden from you specifically": a client renders the name and stops, and MUST NOT
 * infer secrecy, zone, or ownership from the absence.
 */
export interface MomentObject {
  /**
   * The object's opaque entity id — a presentational handle for highlighting only,
   * which may already refer to nothing on the current board.
   */
  id: EntityId;
  /** The display name, fixed when the moment was recorded. */
  name: string;
  /**
   * The retained **public** face, when one was known. Absent otherwise; see the type
   * docs for why that absence carries no information.
   */
  card?: CardView;
}

/**
 * Every {@link MomentKind} tag, mirroring the `MomentKind` enum's `tag = "kind"`
 * snake_case serde encoding. The single source of truth for {@link MomentKindTag} and
 * {@link isMomentKindTag}: a tag added here without an arm in the {@link MomentKind}
 * union fails to compile in the normalizer's exhaustive switch, so the two cannot drift.
 */
export const MOMENT_KINDS = [
  'cast',
  'resolved',
  'countered',
  'fizzled',
  'zone_move',
  'died',
  'damage',
  'life',
  'attacked',
  'blocked',
  'drew',
  'turn_change',
  'phase_change',
  'phases_skipped',
  'eliminated',
  'game_over',
] as const;

/** The tag of a {@link MomentKind}; one of {@link MOMENT_KINDS}. */
export type MomentKindTag = (typeof MOMENT_KINDS)[number];

/** Whether a wire value is a {@link MomentKindTag} this build knows. */
export function isMomentKindTag(value: unknown): value is MomentKindTag {
  return typeof value === 'string' && (MOMENT_KINDS as readonly string[]).includes(value);
}

/**
 * What a {@link PresentationMoment} shows (issue #594).
 *
 * The vocabulary is deliberately **narrower than {@link GameLogEvent}** and answers a
 * different question. The log is the authoritative record of *what happened*, written to
 * be read as prose after the fact; this is the set of things worth giving a beat of
 * screen time to as they happen. So a mulligan or a kept hand has a log entry and no
 * moment (nothing on the board moves), while a zone move has a moment and no distinct
 * log entry (the log names the resolution that caused it).
 *
 * **Counter changes are deliberately absent.** A `+1/+1` counter landing is exactly the
 * kind of small board change this contract exists to give a beat to, but the engine
 * emits no counter-change event and no general zone-change event, and a server that
 * diffed two states to manufacture one would be inventing game information in the very
 * layer whose purpose is to stop clients doing that. A client must not fill the gap
 * either: the honest answer is a missing moment, and when the engine states counter
 * changes a variant arrives here and both ends learn it in the same PR.
 *
 * The union is discriminated on the same `kind` key the Rust enum tags with, and it
 * **widens additively**. A tag this build does not know is unclassified, not an error:
 * {@link normalizePresentationMoments} keeps the moment's place in the stream and marks
 * it {@link PresentationMoment.kindUnknown} rather than coercing it into a known arm.
 */
export type MomentKind =
  /** A player put a spell on the stack (CR 601.2). */
  | { kind: 'cast'; player: PlayerId; object: MomentObject }
  /** A spell or ability finished resolving (CR 608.2), neither countered nor fizzled. */
  | { kind: 'resolved'; player: PlayerId; object: MomentObject }
  /** A spell or ability was countered (CR 701.5) and left the stack without resolving. */
  | { kind: 'countered'; player: PlayerId; object: MomentObject }
  /**
   * A spell or ability left the stack because **all** of its targets had become illegal
   * (CR 608.2b) — the "fizzle". Distinct from `countered`: no effect countered it, so a
   * client that conflates the two credits an opponent with an answer they never had.
   */
  | { kind: 'fizzled'; player: PlayerId; object: MomentObject }
  /**
   * An object changed zones (CR 400.7): the travel a client animates. Both endpoints are
   * stated — see {@link MomentZone} for why the destination alone is not enough and why
   * the client may not work the movement out from a board diff.
   */
  | { kind: 'zone_move'; object: MomentObject; from: MomentZone; to: MomentZone }
  /**
   * A creature **died** — moved from the battlefield to a graveyard (CR 700.4). Only
   * creatures die; any other permanent reaching a graveyard is a `zone_move` and nothing
   * more, the same line {@link GameLogEvent} `permanent_died` draws.
   */
  | { kind: 'died'; object: MomentObject }
  /** Damage was dealt to a player or marked on a permanent (CR 119), lethal or not. */
  | { kind: 'damage'; target: LogDamageTarget; amount: number }
  /**
   * A life total changed by this signed amount from a **non-damage** source (CR 118):
   * life gained, paid, or lost. Damage is the `damage` arm, so the two never
   * double-report one hit.
   */
  | { kind: 'life'; player: PlayerId; amount: number }
  /**
   * A player declared attackers (CR 508.1), carried as the whole declaration rather than
   * one moment per attacker: the beat a player watches is "three creatures attack", and
   * splitting it would spend the window on a list. Possibly empty — a declaration of no
   * attackers is still a declaration.
   */
  | { kind: 'attacked'; player: PlayerId; attackers: MomentObject[] }
  /** A player declared blocker-to-attacker assignments (CR 509.1), carried whole for the
   * same reason `attacked` is. */
  | { kind: 'blocked'; player: PlayerId; blocks: LogBlock[] }
  /**
   * A player drew cards (CR 121.1). Identities are **absent by construction**, exactly as
   * in the log: a draw moment is a count and a player, so no redaction pass is needed to
   * keep it that way.
   */
  | { kind: 'drew'; player: PlayerId; count: number }
  /**
   * The turn passed to a new turn number and/or a new active player (CR 500.1). Emitted
   * in addition to the `phase_change` for the step that opened the turn, because a turn
   * boundary is the one pacing beat worth its own dwell.
   */
  | { kind: 'turn_change'; turn: number; active_player: PlayerId }
  /**
   * The game entered a new step (CR 500.1). The cheapest moment in the vocabulary and the
   * first a client should drop when it is behind — a phase is a caption, and the view's
   * own {@link GameView.phase} is current regardless.
   */
  | { kind: 'phase_change'; phase: Phase }
  /**
   * The room **passed priority for this receiver** at these positions during the settle
   * (issue #455, ADR 0020, CR 117.3) — the moment form of
   * {@link GameView.auto_passed_steps}, so the "we moved without you" beat takes its
   * place in the same ordered stream as the events it happened around.
   *
   * **Per-seat, and the sole reason a receiver's id stream has gaps.** It names positions
   * where *this* receiver was passed; another seat's is that seat's moment, filtered out
   * of this stream. A {@link SpectatorView} carries none at all — there is no seat to
   * speak for.
   *
   * The whole path is **one moment**, never one per priority window, and the ordered path
   * is exactly what {@link AutoPassedStep} already carries (turn included on every entry,
   * because a repeated step means an extra combat phase (CR 506.1) or an extra cleanup
   * (CR 514.3a) at least as often as it means a new turn).
   */
  | { kind: 'phases_skipped'; steps: AutoPassedStep[]; reason: AutoPassReason }
  /**
   * A player left the game mid-game under CR 800.4a — they lost while two or more players
   * remained, so play continues without them. Distinct from `game_over`, which fires only
   * once one player is left; a two-player loss produces `game_over` alone.
   */
  | { kind: 'eliminated'; player: PlayerId; reason: GameOverReason }
  /**
   * The game ended (CR 104.2a); the last moment a room produces. It repeats what
   * {@link GameView.result} already states — the view stays the authority a client
   * renders the verdict from, and this only gives the ending its place in the stream.
   */
  | { kind: 'game_over'; result: GameResult };

/**
 * One **presentation moment**: a single thing that visibly happened, in the order it
 * happened, carried so a client can *pace* what the board already shows (issue #594).
 *
 * **Why this exists at all.** The server applies an action and then settles — resolving
 * the stack, passing priority for idle seats (ADR 0020), advancing steps — before it
 * broadcasts, and the per-seat view channel is latest-value: a view pushed while an
 * earlier one is still in flight replaces it. Both together mean a receiver is handed a
 * *final* board where the game passed through a sequence of causal states, and no amount
 * of client-side diffing recovers the order those states happened in. Diffing two boards
 * says a creature is gone; it cannot say whether it was countered, killed by damage,
 * sacrificed to its own resolution, or exiled and returned. This carries the server's
 * answer instead of inviting the client to invent one.
 *
 * **The same no-loss contract {@link GameView.log} uses.** Every view carries the recent
 * unconsumed suffix, bounded by {@link PRESENTATION_WINDOW}, so nothing depends on a
 * client having seen the previous message. A reconnecting client, a client whose
 * intermediate view was overtaken, and a client that just joined all read this field the
 * same way.
 *
 * **Advisory, display-only, never load-bearing.** The board, the legal actions, and the
 * result are reconstructable from the view *alone*; a client that ignores this field
 * entirely plays exactly the same game. Nothing here may gate applying a view — a
 * presentation delays a *caption*, never the state — and a moment is not a rules event:
 * it does not say an action is legal, that a permanent is still on the battlefield, or
 * that a spell is still on the stack. Those facts live in the view's own fields, always.
 *
 * **Identity, and the gaps in it.** {@link id} is monotonic per room, which makes it a
 * de-duplication key and a watermark: a client stages moments above the highest id it has
 * already shown and discards the rest, so an overlapping window costs nothing. A
 * receiver's stream **may have gaps**, for two independent reasons — the window is
 * bounded, and per-seat moments (a `phases_skipped`, which names where *this* receiver
 * was passed) are filtered out of every other seat's stream. A client therefore MUST NOT
 * treat a missing id as a lost message, wait for it, re-sort the list, or synthesize the
 * hole. Ids are opaque ordering handles; nothing may be derived from their arithmetic
 * beyond "later".
 */
export interface PresentationMoment {
  /**
   * Monotonically increasing per room: the de-duplication key and the ordering authority.
   * A client stages moments in the order given and never re-orders them. A receiver's
   * stream may start well after `1` and may skip values; see the type docs for why
   * neither is an error to recover from.
   */
  id: number;
  /**
   * The **causal group** this moment belongs to: one applied action together with the
   * settle that followed it. Every moment produced by one server step shares a batch id,
   * so a client can tell "these six things happened because of that one click" from
   * "these six things are six separate turns of the crank" — a distinction the timing of
   * arrivals cannot carry, because a whole batch arrives in a single view. Grouping is
   * the only sanctioned use: a batch is not a transaction, not an undo unit, and carries
   * no rules meaning.
   */
  batch: number;
  /**
   * The turn number this moment happened on — **not** necessarily the view's current
   * {@link GameView.turn}, which has already moved on by the time a cross-turn settle is
   * broadcast. A client labels the moment with this and reads the current position from
   * the view.
   */
  turn: number;
  /**
   * The step this moment happened at, with the same caveat as {@link turn}: it is where
   * the game *was*, not where it *is*. A client MUST NOT drive a phase plaque or any
   * other current-position surface from this field.
   */
  phase: Phase;
  /**
   * What happened. Absent when the moment is **unclassified** — the server stated a tag
   * this build does not know — in which case {@link kindUnknown} is set and the moment
   * still holds its place in the ordered stream. A client renders such a moment
   * generically or skips it; it never guesses which known kind was meant.
   */
  kind?: MomentKind;
  /**
   * Normalization state, **not a wire field** (hence camelCase among the snake_case
   * mirror): the server stated a `kind` tag and it is one this build does not know — a
   * variant added to the union after this build, such as a future counter change.
   * {@link normalizePresentationMoments} sets it and leaves {@link kind} unset, so no
   * consumer can mistake an unknown kind for a known one.
   *
   * It exists to *withhold* classification, never to enable one. The moment is kept
   * because its `id`, `batch`, `turn` and `phase` are still readable and dropping it
   * would silently shorten a stream a client paces against; nothing may be inferred from
   * it beyond "something happened here that this build cannot name".
   */
  kindUnknown?: true;
  /**
   * The {@link id} of the moment that **caused** this one, when the server knows it: the
   * resolution a zone move followed from, the death a graveyard move followed from.
   * Stated rather than inferred, because adjacency is not causation — a settle interleaves
   * independent seats' events, so "the moment before" is routinely unrelated. Absent when
   * the cause is unknown or the moment is a root; a client that cannot resolve the
   * referenced id (it fell off the window) simply renders the moment on its own.
   */
  cause?: number;
  /**
   * How many identical occurrences this one moment stands for. The server collapses
   * consecutive moments with an identical {@link kind} — six triggers of the same ability,
   * four instances of the same damage — into one entry with `count` raised, so a repeated
   * event costs one caption ("×6") instead of six dwells that would starve the window of
   * anything worth watching.
   *
   * Always at least `1`. The wire elides it at `1`, which is what an older server and
   * every non-aggregated moment mean, and {@link normalizePresentationMoments}
   * materializes that default so no consumer has to. It is an occurrence tally, **never**
   * an amount: damage and life carry their own magnitudes.
   */
  count: number;
}
