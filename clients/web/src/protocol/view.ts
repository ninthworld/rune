/**
 * In-game view: the personalized game state sent after every change.
 */

import type { PlayerId } from './index.js';
import type { CardView, OpponentView, SelfView, Permanent, StackItem, ZonePile } from './card.js';
import type { ValidAction } from './action.js';
import type { GameResult, CommanderDamage, CommanderTax } from './result.js';
import type { GameLogEntry } from './log.js';

/**
 * Every `Phase` value in turn order, for runtime validation of the wire. This is
 * the single source of truth: the {@link Phase}} union is derived from it, so a
 * value added here can never drift out of the type (and vice versa). Mirrors the
 * `Phase` enum's snake_case serde encoding in `crates/rune-protocol/src/lib.rs`.
 */
export const PHASES = [
  'untap',
  'upkeep',
  'draw',
  'precombat_main',
  'begin_combat',
  'declare_attackers',
  'declare_blockers',
  'combat_damage',
  'end_combat',
  'postcombat_main',
  'end',
  'cleanup',
] as const;

/** The current turn step; one of {@link PHASES}}. */
export type Phase = (typeof PHASES)[number];

/**
 * The personalized state the server sends after every change. A client must be
 * able to fully reconstruct its UI from a single `GameView` — no client state is
 * load-bearing across messages. Optional collections may be omitted on the wire
 * and MUST be treated as their empty default (see {@link normalizeGameView}}).
 */
export interface GameView {
  /**
   * The receiver's own seat entity id (the `p{N}` form used for players
   * throughout the view). The client uses this to identify itself rather than
   * inferring it from the zones. An older server may omit it on the wire, in
   * which case {@link normalizeGameView}} defaults it to `''`.
   */
  you: PlayerId;
  /** Full card objects for the receiving player only. */
  my_hand: CardView[];
  /**
   * The receiver's own public stats (life total, library size) — see
   * {@link SelfView}}. An older server may omit it; {@link normalizeGameView}}
   * defaults it to a zero placeholder.
   */
  me: SelfView;
  /** Redacted views of every other player. */
  opponents: OpponentView[];
  /** All permanents in play. */
  battlefield: Permanent[];
  /** The stack, bottom first. */
  stack: StackItem[];
  /** Each player's graveyard. */
  graveyards: ZonePile[];
  /** Each player's exile zone. */
  exile: ZonePile[];
  /**
   * Each player's command zone (CR 903.6, issue #372): the public pile holding
   * their commander while it is there. Public information; {@link normalizeGameView}}
   * always sets it (to `[]` when the wire omits it for a non-commander game).
   * Optional on the interface so existing view literals need not restate it.
   */
  command?: ZonePile[];
  /** The current turn step. */
  phase: Phase;
  /**
   * The current turn number (1-based; `0` only in an empty/pre-game state). The
   * server owns turn counting — the client renders this and never counts turns
   * itself. Defaulted to `0` by {@link normalizeGameView}} when an older server
   * omits it.
   */
  turn: number;
  /**
   * The player whose turn it is (the active player), as a `p{N}` id. Distinct from
   * {@link GameView.priority_player}}: the active player owns the turn even while an
   * opponent holds priority. Defaulted to `''` (unknown) when an older server omits
   * it.
   */
  active_player: PlayerId;
  /**
   * The table's seat order: every player id (`p0`, `p1`, …) in seat order, including
   * the receiver and any eliminated players (issue #345). The explicit ordering the
   * multiplayer table layout relies on. Defaulted to `[]` by {@link normalizeGameView}}
   * when an older server omits it.
   */
  seat_order: PlayerId[];
  /** The receiving player's unspent mana, as pip strings (e.g. `["{G}"]`). */
  mana_pool: string[];
  /** The player who currently holds priority, if any. */
  priority_player?: PlayerId;
  /** The only source of interactivity: what the receiving player may do now. */
  valid_actions: ValidAction[];
  /** Seconds remaining for the pending decision, if a clock is running. */
  action_deadline?: number;
  /**
   * The terminal result once the game is over (winner/losers/reason, CR 104.2a).
   * Absent while the game is live (the empty-optional convention), so its presence
   * alone tells a client the game has ended; when present, `valid_actions` is empty.
   */
  result?: GameResult;
  /** Bounded structured log window; older servers may omit it. */
  log?: GameLogEntry[];
  /**
   * The receiver's own current **priority-stop preferences** (issue #264, ADR 0020):
   * the steps at which they want priority even when the engine reports no meaningful
   * action, so basic auto-pass does not skip them there. Carried on the view so the
   * per-phase stops UI is reconstructable from a single message and survives reconnect
   * (the preferences live on the server, not in client memory). The client renders
   * toggles from this and answers with a {@link SetStopsMessage}}. Omitted (treated as
   * empty — "stop nowhere", the default) by the server when empty;
   * {@link normalizeGameView}} defaults it to `[]`.
   */
  stops?: Phase[];
  /**
   * Whether reaching this state **auto-passed** priority on the receiver's behalf
   * (issue #264, ADR 0020): set on the broadcast that follows a settle in which the
   * server passed priority for this seat, so the client can show a display-only
   * "passed for you" indicator. Advisory and transient — the UI reconstructs fully
   * without it. Omitted (treated as `false`) when the seat was not auto-passed.
   */
  auto_passed?: boolean;
  /**
   * Whether this view answers a **rejected in-game action** by the receiver (issue
   * #265): a stale-view race meant the chosen action was no longer on offer, so the
   * server re-sent the current state unchanged and flagged that one re-send. Advisory
   * and transient like {@link auto_passed}} — `valid_actions` already reflects the true
   * legal set, so the client shows only a brief, non-blaming "the game moved on" toast
   * (ephemeral presentation, never load-bearing). Omitted (treated as `false`) on every
   * normal broadcast and resync; {@link normalizeGameView}} defaults it to `false`.
   */
  action_rejected?: boolean;
  /**
   * Public display names keyed by {@link PlayerId}} (issue #294): every player who has
   * chosen a name maps to it, so any in-game surface (turn indicator, player tiles,
   * zone-browser titles, game-over verdict) can label any player — `you`, an opponent,
   * the active/priority player, a winner — without a lobby round-trip. Names never
   * replace the `p{N}` id an action echoes back. A player with no name has no entry;
   * the server omits the field entirely when empty and {@link normalizeGameView}}
   * defaults it to `{}`, so an older server that never sends names keeps working.
   */
  player_names: Record<PlayerId, string>;
  /**
   * Cumulative commander combat damage per `(commander, damaged)` pair (CR 903.10a,
   * issue #371) — public information, the same for every receiver (see
   * {@link CommanderDamage}}). A player who has taken 21+ from one commander has lost
   * (shown in {@link GameView.result}} with reason `commander_damage`); the running
   * tally lets the client warn before then. Omitted (treated as `[]`) in a
   * non-commander game or from an older server; {@link normalizeGameView}} defaults
   * it. Server-computed; never derived by the client.
   */
  commander_damage: CommanderDamage[];
  /**
   * The commander tax owed on each designated commander (CR 903.8, issue #372) —
   * public information (see {@link CommanderTax}}). {@link normalizeGameView}}
   * always sets it (to `[]` when omitted). Optional on the interface so existing view
   * literals need not restate it.
   */
  commander_tax?: CommanderTax[];
}
