/**
 * Spectator view: redacted public game state for non-seated observers.
 */

import type { PlayerId } from './index.js';
import type { OpponentView, Permanent, StackItem, ZonePile } from './card.js';
import type { Phase } from './view.js';
import type { GameResult, CommanderDamage, CommanderTax } from './result.js';
import type { GameLogEntry } from './log.js';

/**
 * The state a **spectator** connection receives (ADR 0022, issue #351): a non-seated
 * observer watching a live game with all hidden information redacted **by
 * construction**. It reuses {@link GameView}}'s public component types but carries no
 * receiver fields — there is no `you`, `me`, `my_hand`, `mana_pool`, `valid_actions`,
 * `action_deadline`, or per-seat prompt, so the client's spectate mode can never even
 * read a hand or a decision surface. Every seat is a public {@link OpponentView}}; a
 * spectator reconstructs the whole public board from a single `SpectatorView`.
 */
export interface SpectatorView {
  /** Every player at the table as public state — no privileged "self". */
  players: OpponentView[];
  /** All permanents in play. */
  battlefield: Permanent[];
  /** The stack, bottom first. */
  stack: StackItem[];
  /** Each player's public graveyard. */
  graveyards: ZonePile[];
  /** Each player's public exile zone. */
  exile: ZonePile[];
  /**
   * Each player's public command zone (CR 903.6, issue #372) — the same public pile
   * seated views carry. {@link normalizeSpectatorView}} always sets it (to `[]` when
   * omitted for a non-commander game).
   */
  command?: ZonePile[];
  /** The current turn step. */
  phase: Phase;
  /** The current turn number (1-based). */
  turn: number;
  /** The player whose turn it is. */
  active_player: PlayerId;
  /** Every seat's id in seat order, including eliminated players. */
  seat_order: PlayerId[];
  /** The player currently holding priority, if any (whose turn it is to act). */
  priority_player?: PlayerId;
  /** The terminal result once the game is over; absent while live. */
  result?: GameResult;
  /** Bounded structured public log window. */
  log?: GameLogEntry[];
  /** Public display names keyed by player id. */
  player_names: Record<PlayerId, string>;
  /**
   * Cumulative commander combat damage per `(commander, damaged)` pair (CR 903.10a,
   * issue #371) — the same public tally seated views carry (see
   * {@link CommanderDamage}}). Omitted (treated as `[]`) in a non-commander game.
   */
  commander_damage: CommanderDamage[];
  /**
   * The commander tax owed on each designated commander (CR 903.8, issue #372) — the
   * same public projection seated views carry. {@link normalizeSpectatorView}}
   * always sets it (to `[]` when omitted for a non-commander game).
   */
  commander_tax?: CommanderTax[];
}
