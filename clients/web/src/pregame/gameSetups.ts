/**
 * The game-setup choices the create form offers, and the label an opaque
 * `game_setup` id renders as (issue #506, carried unchanged from the shipped
 * lobby).
 *
 * The `game_setup` id is **opaque to the client** and validated server-side
 * (ADR 0013 vocabulary); these are only the choices a player picks from and the
 * human labels those ids render as. An unknown setup falls back to its raw id,
 * so a newer server's format still renders rather than going blank.
 */
import type { RoomConfig, RoomVisibility } from '../protocol';

/** A game-setup option offered by the create-room form. */
export interface GameSetupOption {
  /** The opaque `game_setup` id sent to the server. */
  readonly id: string;
  /** Display label. */
  readonly label: string;
  /** The seat count this setup is designed for (pre-fills the seat picker). */
  readonly seats: number;
}

/** The setups the create form offers. */
export const GAME_SETUPS: readonly GameSetupOption[] = [
  { id: '1v1', label: '1v1 Duel', seats: 2 },
  { id: 'ffa-4', label: 'Free-for-all (4)', seats: 4 },
  { id: 'commander', label: 'Commander', seats: 4 },
];

/** The seat counts the lobby offers, matching the protocol's `2..=8` range. */
export const SEAT_COUNTS = [2, 3, 4, 5, 6, 7, 8] as const;

/** A human label for an opaque `game_setup` id, falling back to the raw id. */
export function setupLabel(gameSetup: string): string {
  return GAME_SETUPS.find((option) => option.id === gameSetup)?.label ?? gameSetup;
}

/**
 * What to call a table (issue #546): the host's chosen `RoomConfig.name`, or — when the
 * table is unnamed — the label of its format.
 *
 * The fallback is deliberately the client's, not the server's: `RoomConfig.name` is
 * absent rather than defaulted on the wire precisely so no display prose rides the
 * protocol, and so a room created before the field existed still reads exactly as it
 * always did.
 */
export function tableName(config: RoomConfig): string {
  const named = config.name?.trim() ?? '';
  return named.length > 0 ? named : setupLabel(config.game_setup);
}

/**
 * The word for a table's listing rule (issue #546). Rendered as a word, never as a
 * colour or an icon alone — `front-door-and-lobby.md` §5.9's non-colour-channel rule.
 */
export function visibilityLabel(visibility: RoomVisibility | undefined): string {
  return visibility === 'private' ? 'Private' : 'Public';
}
