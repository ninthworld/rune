/**
 * Game-log prose composition and grouping (issue #260) — the pure, presentation-only
 * core the {@link './GameLog'.GameLog} panel renders.
 *
 * The server never sends log prose: it sends structured {@link GameLogEvent}s (a
 * bounded window on {@link GameView.log}, ADR 0021 / docs/protocol.md §Game log), and
 * the client composes the readable sentence itself — the one place a client turns
 * engine facts into words, exactly as it renders a card from its fields. No game logic
 * is derived here (AGENTS.md hard rule): every value shown comes straight off the event,
 * and every entity/player reference keeps the *server-supplied* name, which the protocol
 * fixes at record time (a permanent's name in an old entry stays stable after it leaves
 * play — the server does not re-resolve it).
 *
 * Two pure transforms, both total and free of accumulated state so the panel stays
 * reconstructable from a single view:
 *
 * - {@link describeEvent} turns one event into an ordered list of {@link LogSegment}s —
 *   plain text interleaved with {@link LogRef} references the panel makes clickable for
 *   presentational highlighting.
 * - {@link groupEntries} folds a run of consecutive `step_changed` entries (the
 *   repetitive turn/phase "spam" this event set produces) into one collapsible
 *   {@link StepsGroup}, leaving every other entry on its own line (ui-requirements
 *   §Comprehension: collapse repetitive sequences).
 */
import type { GameLogEntry, GameLogEvent, GameResult, Phase, PlayerId } from '../protocol';
import { playerName } from '../playerNames';

/** A clickable reference inside a composed log line: a permanent/spell entity or a
 * player. Carries the opaque id (for presentational highlighting only) and the display
 * name to render. */
export type LogRef =
  { kind: 'entity'; id: string; name: string } | { kind: 'player'; id: PlayerId; name: string };

/** One piece of a composed log line: literal text, or a clickable {@link LogRef}. */
export type LogSegment = string | LogRef;

/** Whether a segment is a clickable reference (vs literal text). */
export function isRef(segment: LogSegment): segment is LogRef {
  return typeof segment !== 'string';
}

/** Title-case a snake_case {@link Phase} for display, e.g. `precombat_main` →
 * `Precombat Main`. Pure formatting — never parsed for meaning. */
export function phaseLabel(phase: Phase): string {
  return phase
    .split('_')
    .map((word) => word.charAt(0).toUpperCase() + word.slice(1))
    .join(' ');
}

/** A human-readable clause for why a game ended (display only; CR 104/704.5). */
function reasonClause(result: GameResult): string {
  switch (result.reason) {
    case 'life_zero':
      return 'life total reached zero';
    case 'decked':
      return 'drew from an empty library';
    case 'concede':
      return 'conceded';
    default:
      // Tolerate an unknown future reason without inventing meaning.
      return result.reason;
  }
}

/** The view fields the composer needs: the player-name map (for {@link playerName}). */
type NamingView = Parameters<typeof playerName>[0];

/** A player reference segment, named via the view's display-name map. */
function player(view: NamingView, id: PlayerId): LogRef {
  return { kind: 'player', id, name: playerName(view, id) };
}

/** An entity reference segment, using the event's server-supplied (record-time) name. */
function entity(ref: { id: string; name: string }): LogRef {
  return { kind: 'entity', id: ref.id, name: ref.name };
}

/** Interleave a separator between refs to build a natural list (`a`, `a and b`,
 * `a, b, and c`). Returns the flat segment list ready to splice into a line. */
function refList(refs: LogRef[]): LogSegment[] {
  if (refs.length === 0) return [];
  if (refs.length === 1) return [refs[0]];
  if (refs.length === 2) return [refs[0], ' and ', refs[1]];
  const out: LogSegment[] = [];
  refs.forEach((ref, i) => {
    if (i > 0) out.push(i === refs.length - 1 ? ', and ' : ', ');
    out.push(ref);
  });
  return out;
}

/**
 * Compose one structured event into an ordered list of display segments. Pure: the
 * same event + naming view always yields the same segments, and nothing here derives
 * legality or state — it only formats the fields the server already sent.
 */
export function describeEvent(event: GameLogEvent, view: NamingView): LogSegment[] {
  switch (event.type) {
    case 'spell_cast':
      return [player(view, event.player), ' cast ', entity(event.card), '.'];
    case 'spell_resolved':
      return [entity(event.card), ' resolved.'];
    case 'spell_countered':
      return [entity(event.card), ' was countered.'];
    case 'spell_fizzled':
      return [entity(event.card), ' fizzled.'];
    case 'attackers_declared':
      return event.attackers.length === 0
        ? [player(view, event.player), ' declares no attackers.']
        : [
            player(view, event.player),
            ' attacks with ',
            ...refList(event.attackers.map(entity)),
            '.',
          ];
    case 'blockers_declared': {
      if (event.blocks.length === 0) {
        return [player(view, event.player), ' declares no blockers.'];
      }
      const out: LogSegment[] = [player(view, event.player), ' blocks: '];
      event.blocks.forEach((block, i) => {
        if (i > 0) out.push('; ');
        out.push(entity(block.blocker), ' blocks ', entity(block.attacker));
      });
      out.push('.');
      return out;
    }
    case 'mulligan':
      return [player(view, event.player), ' mulligans.'];
    case 'hand_kept':
      return [player(view, event.player), ' keeps their hand.'];
    case 'life_changed': {
      const magnitude = Math.abs(event.amount);
      const verb = event.amount >= 0 ? 'gains' : 'loses';
      return [player(view, event.player), ` ${verb} ${magnitude} life.`];
    }
    case 'damage_dealt': {
      const target: LogSegment =
        event.target.kind === 'player'
          ? player(view, event.target.player)
          : entity(event.target.permanent);
      return [target, ` takes ${event.amount} damage.`];
    }
    case 'cards_drawn':
      return [
        player(view, event.player),
        ` draws ${event.count} ${event.count === 1 ? 'card' : 'cards'}.`,
      ];
    case 'permanent_died':
      return [entity(event.permanent), ' died.'];
    case 'step_changed':
      return [
        `Turn ${event.turn}, ${phaseLabel(event.phase)} — `,
        player(view, event.active_player),
      ];
    case 'game_over':
      return event.result.winner !== undefined
        ? [
            'Game over — ',
            player(view, event.result.winner),
            ` wins (${reasonClause(event.result)}).`,
          ]
        : [`Game over — draw (${reasonClause(event.result)}).`];
    default:
      // An unknown future event kind degrades to nothing rather than crashing the
      // whole panel (forward compatibility, mirroring the wire's tolerance elsewhere).
      return [];
  }
}

/** A single entry rendered on its own line. */
export interface EntryGroup {
  kind: 'entry';
  entry: GameLogEntry;
}

/** A collapsed run (≥2) of consecutive `step_changed` entries — the repetitive
 * turn/phase advance the panel folds behind one expandable summary. */
export interface StepsGroup {
  kind: 'steps';
  entries: GameLogEntry[];
}

/** One rendered group: a standalone entry or a collapsed step run. */
export type LogGroup = EntryGroup | StepsGroup;

/**
 * Fold consecutive `step_changed` entries into collapsible {@link StepsGroup}s (a run
 * of two or more), leaving every other entry — and any lone step change — as its own
 * {@link EntryGroup}. Order is preserved. Pure and total: a fresh call over the same
 * window yields the same groups, so nothing is load-bearing across renders.
 */
export function groupEntries(entries: GameLogEntry[]): LogGroup[] {
  const groups: LogGroup[] = [];
  let run: GameLogEntry[] = [];
  const flush = (): void => {
    if (run.length === 0) return;
    if (run.length >= 2) groups.push({ kind: 'steps', entries: run });
    else groups.push({ kind: 'entry', entry: run[0] });
    run = [];
  };
  for (const entry of entries) {
    if (entry.event.type === 'step_changed') {
      run.push(entry);
    } else {
      flush();
      groups.push({ kind: 'entry', entry });
    }
  }
  flush();
  return groups;
}
