/**
 * Game-end outcomes, commander tracking, and related contract types.
 */

import type { PlayerId } from './index.js';

/**
 * One player's cumulative **combat** damage from one commander this game
 * (CR 903.10a, issue #371), as carried in {@link GameView.commander_damage}}.
 * Public information — every player and spectator sees the same tally. The
 * commander is named by its owning player's id (one commander per player today),
 * the stable key that survives the commander's zone changes; the client renders 21
 * as the lethal threshold. Server-computed; never derived by the client.
 */
export interface CommanderDamage {
  /** The commander that dealt the damage, as its owning player's id. */
  commander: PlayerId;
  /** The player that has taken the damage. */
  damaged: PlayerId;
  /** Cumulative combat damage this commander has dealt this player this game. */
  amount: number;
}

/**
 * The **commander tax** owed on one player's commander (CR 903.8, issue #372), as
 * carried in {@link GameView.commander_tax}}. Public information — the tax climbs
 * `{2}` per prior cast from the command zone, so every seat sees a recast's cost.
 * Server-computed; never derived by the client.
 */
export interface CommanderTax {
  /** The commander this tax applies to, as its owning player's id. */
  commander: PlayerId;
  /** How many times this commander has been cast from the command zone this game. */
  casts?: number;
  /** Generic mana the tax adds to the next cast (`2 * casts`); `0`/omitted at first. */
  tax?: number;
}

/**
 * Every {@link GameOverReason}} value, mirroring the `GameOverReason` enum's
 * snake_case serde encoding in `crates/rune-protocol/src/lib.rs`. This is the
 * single source of truth: the {@link GameOverReason}} union is derived from it and
 * {@link isGameOverReason}} validates a wire value against it, so a reason added
 * here can never drift out of the type (and vice versa).
 */
export const GAME_OVER_REASONS = ['life_zero', 'decked', 'concede', 'commander_damage'] as const;

/**
 * Why a game ended (CR 104.3 / CR 704.5 / CR 903.10a), a closed snake_case enum;
 * one of {@link GAME_OVER_REASONS}}:
 * - `life_zero` — a player was reduced to 0 or less life (CR 704.5a).
 * - `decked` — a player attempted to draw from an empty library (CR 704.5c).
 * - `concede` — a player conceded (CR 104.3a).
 * - `commander_damage` — a player took 21+ combat damage from one commander
 *   (CR 903.10a).
 */
export type GameOverReason = (typeof GAME_OVER_REASONS)[number];

/** Whether a wire value is a known {@link GameOverReason}}. */
export function isGameOverReason(value: unknown): value is GameOverReason {
  return typeof value === 'string' && (GAME_OVER_REASONS as readonly string[]).includes(value);
}

/**
 * The terminal outcome of a game (CR 104.2a), present on a {@link GameView}} only
 * once the game is over. While the game is live the field is omitted entirely (the
 * empty-optional convention), so its mere presence signals game over to a client.
 * The client renders this; it never decides a winner or terminality itself.
 */
export interface GameResult {
  /**
   * The winning player (CR 104.2a). Absent for a draw, where every remaining
   * player lost at once (CR 104.4a).
   */
  winner?: PlayerId;
  /** The players who lost, in seat order. */
  losers: PlayerId[];
  /** Why the game ended. */
  reason: GameOverReason;
}
