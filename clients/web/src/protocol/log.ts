/**
 * Structured game-history events for the log window.
 */

import type { EntityId, PlayerId } from './index.js';
import type { Phase } from './view.js';
import type { GameOverReason, GameResult } from './result.js';

/** A named entity reference carried by a structured log event. */
export interface LogEntity {
  /** Opaque entity id. */
  id: EntityId;
  /** Server-supplied display name. */
  name: string;
}

/** One declared blocker assignment. */
export interface LogBlock {
  /** Blocking permanent. */
  blocker: LogEntity;
  /** Attacked permanent. */
  attacker: LogEntity;
}

/** What a `damage_dealt` event was dealt to: a player or a permanent. */
export type LogDamageTarget =
  { kind: 'player'; player: PlayerId } | { kind: 'permanent'; permanent: LogEntity };

/** A structured, receiver-safe game-history event. */
export type GameLogEvent =
  | { type: 'spell_cast'; player: PlayerId; card: LogEntity }
  | { type: 'spell_resolved'; player: PlayerId; card: LogEntity }
  | { type: 'spell_countered'; player: PlayerId; card: LogEntity }
  | { type: 'spell_fizzled'; player: PlayerId; card: LogEntity }
  | { type: 'attackers_declared'; player: PlayerId; attackers: LogEntity[] }
  | { type: 'blockers_declared'; player: PlayerId; blocks: LogBlock[] }
  | { type: 'mulligan'; player: PlayerId }
  | { type: 'hand_kept'; player: PlayerId }
  | { type: 'life_changed'; player: PlayerId; amount: number }
  | { type: 'damage_dealt'; target: LogDamageTarget; amount: number }
  | { type: 'cards_drawn'; player: PlayerId; count: number }
  | { type: 'permanent_died'; permanent: LogEntity }
  | { type: 'step_changed'; turn: number; active_player: PlayerId; phase: Phase }
  | { type: 'player_eliminated'; player: PlayerId; reason: GameOverReason }
  | { type: 'commander_returned_to_command_zone'; player: PlayerId; card: LogEntity }
  | { type: 'game_over'; result: GameResult };

/** One sequence-numbered entry in the authoritative recent game-history window. */
export interface GameLogEntry {
  /** Monotonically increasing server sequence number. */
  sequence: number;
  /** Event payload rendered as client-local prose. */
  event: GameLogEvent;
}
