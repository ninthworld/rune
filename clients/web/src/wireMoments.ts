/**
 * Wire parsing for the **presentation window** (issue #594): the bounded, ordered list
 * of {@link PresentationMoment}s that rides every {@link GameView} and
 * {@link SpectatorView}.
 *
 * Split out of {@link ./wire} to keep that module inside the file-size budget AGENTS.md
 * sets; `wire.ts` re-exports {@link normalizePresentationMoments} and calls it from both
 * view normalizers. It also owns {@link normalizeAutoPassedSteps}, because the
 * `phases_skipped` moment carries exactly the path {@link GameView.auto_passed_steps}
 * does and both must be read the same way.
 *
 * **This module classifies nothing.** Every kind, zone, reason, and cause on the wire is
 * the server's own statement; the reader validates the shape and drops what it cannot
 * read. It never guesses a kind from a payload, never reconstructs a moment from the
 * board, never sorts, never merges, and never de-duplicates — order and repetition are
 * the payload, exactly as they are for the auto-passed path.
 */
import {
  type AutoPassedStep,
  type CardView,
  type GameOverReason,
  type LogBlock,
  type LogDamageTarget,
  type LogEntity,
  type MomentKind,
  type MomentKindTag,
  type MomentObject,
  type PlayerId,
  type PresentationMoment,
  isAutoPassReason,
  isMomentKindTag,
  isMomentZone,
  isPhase,
} from './protocol';
import { isFiniteNumber, isRecord, normalizeGameResult } from './wirePrimitives';

/**
 * Coerce a wire value into `GameView.auto_passed_steps` (issue #455) — and into the
 * `steps` of a `phases_skipped` moment (issue #594), which is the same path in the same
 * order: the ordered positions a settle carried the receiver through.
 *
 * **Order and repeats are the payload**, so this only drops entries it cannot read — it
 * never sorts, never merges, and never de-duplicates. A settle can revisit a step both
 * across a turn boundary and inside one turn (an extra combat phase, CR 506.1), and
 * collapsing those occurrences would silently under-report how far the game moved. An
 * entry missing a usable `turn` is dropped rather than defaulted, because a wrong turn
 * would place a real skip on the wrong turn's step list — worse than omitting it.
 */
export function normalizeAutoPassedSteps(value: unknown): AutoPassedStep[] {
  if (!Array.isArray(value)) return [];
  const steps: AutoPassedStep[] = [];
  for (const entry of value) {
    if (!isRecord(entry)) continue;
    const { phase, turn } = entry;
    if (!isPhase(phase)) continue;
    if (!isFiniteNumber(turn)) continue;
    steps.push({ phase, turn });
  }
  return steps;
}

/**
 * Normalize one {@link MomentObject}: the snapshot a moment retains of an object that is
 * by definition no longer where it was. Both the id and the name are required — the name
 * is the whole reason the snapshot exists, and a nameless reference would resolve to
 * nothing on the current board — so an entry missing either is unreadable and its moment
 * is dropped rather than captioned with a blank.
 *
 * The retained `card` rides through untouched, like every other server-computed face. It
 * is a **public** face by construction (see {@link MomentObject}); the client neither
 * checks nor supplies one, and must not read anything into its absence.
 */
function normalizeMomentObject(raw: unknown): MomentObject | undefined {
  if (!isRecord(raw)) return undefined;
  if (typeof raw.id !== 'string' || typeof raw.name !== 'string') return undefined;
  const object: MomentObject = { id: raw.id, name: raw.name };
  // (`as unknown as` because the record guard has already narrowed the value away from
  // `unknown`; the face itself is carried verbatim, never reshaped.)
  if (isRecord(raw.card)) object.card = raw.card as unknown as CardView;
  return object;
}

/**
 * Normalize a list of retained objects (the attackers of an `attacked` moment), keeping
 * declaration order and dropping any entry that cannot be read. An empty list is normal,
 * not an error: a declaration of no attackers is still a declaration.
 */
function normalizeMomentObjects(value: unknown): MomentObject[] {
  if (!Array.isArray(value)) return [];
  return value
    .map(normalizeMomentObject)
    .filter((object): object is MomentObject => object !== undefined);
}

/** Normalize one `{ id, name }` log entity; both halves are required. */
function normalizeLogEntity(raw: unknown): LogEntity | undefined {
  if (!isRecord(raw)) return undefined;
  if (typeof raw.id !== 'string' || typeof raw.name !== 'string') return undefined;
  return { id: raw.id, name: raw.name };
}

/**
 * Normalize the blocker-to-attacker assignments of a `blocked` moment (CR 509.1) — the
 * same pairs the log carries. A pair missing either half names no relationship and is
 * dropped; the client never re-pairs a blocker with an attacker itself.
 */
function normalizeLogBlocks(value: unknown): LogBlock[] {
  if (!Array.isArray(value)) return [];
  const blocks: LogBlock[] = [];
  for (const raw of value) {
    if (!isRecord(raw)) continue;
    const blocker = normalizeLogEntity(raw.blocker);
    const attacker = normalizeLogEntity(raw.attacker);
    if (blocker === undefined || attacker === undefined) continue;
    blocks.push({ blocker, attacker });
  }
  return blocks;
}

/**
 * Normalize what a `damage` moment was dealt to (CR 119). The `kind` tag is the server's
 * own classification of the target, so this only validates it and the value beside it —
 * the client never classifies a bare id by testing which collection it appears in, the
 * same rule {@link normalizeStackTarget} follows.
 */
function normalizeDamageTarget(raw: unknown): LogDamageTarget | undefined {
  if (!isRecord(raw)) return undefined;
  if (raw.kind === 'player') {
    return typeof raw.player === 'string' ? { kind: 'player', player: raw.player } : undefined;
  }
  if (raw.kind === 'permanent') {
    const permanent = normalizeLogEntity(raw.permanent);
    return permanent === undefined ? undefined : { kind: 'permanent', permanent };
  }
  return undefined;
}

/** The `{ player, object }` shape the four stack-life-cycle kinds share. */
function normalizePlayerObject(
  raw: Record<string, unknown>,
): { player: PlayerId; object: MomentObject } | undefined {
  if (typeof raw.player !== 'string') return undefined;
  const object = normalizeMomentObject(raw.object);
  return object === undefined ? undefined : { player: raw.player, object };
}

/**
 * Normalize the payload of a moment whose `kind` tag this build knows, returning
 * `undefined` when the payload is unreadable.
 *
 * A **known tag with an unreadable payload is dropped, not defaulted**: a `damage`
 * moment with no amount, a `zone_move` with an unrecognized endpoint, a `phases_skipped`
 * with an unstated reason all name a beat this client cannot honestly caption, and
 * inventing the missing half would be the client asserting game structure the server
 * never sent. That is a different case from a tag this build has never heard of, which
 * the caller keeps as unclassified — see {@link PresentationMoment.kindUnknown}.
 *
 * The switch is exhaustive over {@link MomentKindTag}: a tag added to `MOMENT_KINDS`
 * without an arm here fails to compile, which is how the vocabulary and the reader are
 * kept from drifting apart.
 */
function normalizeMomentKind(
  raw: Record<string, unknown>,
  tag: MomentKindTag,
): MomentKind | undefined {
  switch (tag) {
    case 'cast':
    case 'resolved':
    case 'countered':
    case 'fizzled': {
      const payload = normalizePlayerObject(raw);
      return payload === undefined ? undefined : { kind: tag, ...payload };
    }
    case 'zone_move': {
      const object = normalizeMomentObject(raw.object);
      if (object === undefined || !isMomentZone(raw.from) || !isMomentZone(raw.to)) {
        return undefined;
      }
      return { kind: 'zone_move', object, from: raw.from, to: raw.to };
    }
    case 'died': {
      const object = normalizeMomentObject(raw.object);
      return object === undefined ? undefined : { kind: 'died', object };
    }
    case 'damage': {
      const target = normalizeDamageTarget(raw.target);
      if (target === undefined || !isFiniteNumber(raw.amount)) return undefined;
      return { kind: 'damage', target, amount: raw.amount };
    }
    case 'life': {
      if (typeof raw.player !== 'string' || !isFiniteNumber(raw.amount)) return undefined;
      return { kind: 'life', player: raw.player, amount: raw.amount };
    }
    case 'attacked': {
      if (typeof raw.player !== 'string') return undefined;
      return {
        kind: 'attacked',
        player: raw.player,
        attackers: normalizeMomentObjects(raw.attackers),
      };
    }
    case 'blocked': {
      if (typeof raw.player !== 'string') return undefined;
      return { kind: 'blocked', player: raw.player, blocks: normalizeLogBlocks(raw.blocks) };
    }
    case 'drew': {
      if (typeof raw.player !== 'string' || !isFiniteNumber(raw.count)) return undefined;
      return { kind: 'drew', player: raw.player, count: raw.count };
    }
    case 'turn_change': {
      if (!isFiniteNumber(raw.turn) || typeof raw.active_player !== 'string') return undefined;
      return { kind: 'turn_change', turn: raw.turn, active_player: raw.active_player };
    }
    case 'phase_change': {
      return isPhase(raw.phase) ? { kind: 'phase_change', phase: raw.phase } : undefined;
    }
    case 'phases_skipped': {
      // The reason is never guessed: "we had nothing for you to do" and "you had no legal
      // declaration to make" are two different sentences, and choosing one for the other
      // would be the client deriving automation policy it cannot see (ADR 0020).
      if (!isAutoPassReason(raw.reason)) return undefined;
      return {
        kind: 'phases_skipped',
        steps: normalizeAutoPassedSteps(raw.steps),
        reason: raw.reason,
      };
    }
    case 'eliminated': {
      // The reason rides through verbatim, exactly as it does in a terminal `result`:
      // an unrecognized future value is rendered generically, never re-derived.
      if (typeof raw.player !== 'string' || typeof raw.reason !== 'string') return undefined;
      return { kind: 'eliminated', player: raw.player, reason: raw.reason as GameOverReason };
    }
    case 'game_over': {
      const result = normalizeGameResult(raw.result);
      return result === undefined ? undefined : { kind: 'game_over', result };
    }
  }
}

/**
 * Coerce a wire value into the ordered presentation window (issue #594).
 *
 * **The order on the wire is the order that happened.** This reader preserves it exactly:
 * it never sorts by `id`, never merges adjacent moments, never de-duplicates, and never
 * fills a gap in the id sequence — a receiver's stream legitimately starts late and skips
 * values (the window is bounded, and another seat's `phases_skipped` is filtered out of
 * this one), so a hole is not a lost message and nothing here may treat it as one.
 *
 * Three readings, in order of how much the payload gave us:
 * - An entry whose **frame** is unreadable — no numeric `id`/`batch`/`turn`, no known
 *   `phase`, no `kind` object — is **dropped**. It cannot be ordered, de-duplicated, or
 *   labelled, and a defaulted id would collide with a real one and suppress it.
 * - An entry whose `kind` tag is known but whose payload is unreadable is **dropped**;
 *   see {@link normalizeMomentKind}.
 * - An entry whose `kind` tag is a value this build does not know is **kept, unclassified**:
 *   `kind` stays unset and {@link PresentationMoment.kindUnknown} is set, so it holds its
 *   place in the stream without any consumer mistaking it for a known kind.
 *
 * `count` materializes the wire's elided `1` (and refuses a non-positive or non-finite
 * value, which would mean a caption of "×0"); `cause` stays absent unless the server
 * stated one, because a guessed cause would assert a causal link from mere adjacency —
 * the exact inference this field exists to make unnecessary.
 */
export function normalizePresentationMoments(value: unknown): PresentationMoment[] {
  if (!Array.isArray(value)) return [];
  const moments: PresentationMoment[] = [];
  for (const raw of value) {
    if (!isRecord(raw)) continue;
    const { id, batch, turn, phase, kind } = raw;
    if (!isFiniteNumber(id) || !isFiniteNumber(batch) || !isFiniteNumber(turn)) continue;
    if (!isPhase(phase)) continue;
    if (!isRecord(kind) || typeof kind.kind !== 'string') continue;
    const moment: PresentationMoment = {
      id,
      batch,
      turn,
      phase,
      count: isFiniteNumber(raw.count) && raw.count >= 1 ? raw.count : 1,
    };
    if (isFiniteNumber(raw.cause)) moment.cause = raw.cause;
    if (isMomentKindTag(kind.kind)) {
      const classified = normalizeMomentKind(kind, kind.kind);
      if (classified === undefined) continue;
      moment.kind = classified;
    } else {
      moment.kindUnknown = true;
    }
    moments.push(moment);
  }
  return moments;
}
