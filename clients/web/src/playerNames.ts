/**
 * Player display-name resolution (issue #294).
 *
 * Every in-game and lobby surface labels players by their chosen display name when
 * the server sends one, and falls back gracefully when it does not — so an older
 * server that never sends names keeps working. This is pure presentation: the
 * client never parses the opaque `p{N}` id to derive meaning (AGENTS.md / protocol
 * "entity ids are opaque"), it only looks a name up or uses a field the server
 * already provides (the roster's real seat index).
 */
import type { GameView, PlayerId, SeatView } from './protocol';

/**
 * The display name for a player in a {@link GameView}: their chosen name when the
 * server's {@link GameView.player_names} map carries one, else the raw opaque id
 * (the pre-names behavior — never parsed, just shown). Accepts any object exposing
 * `player_names` so callers can pass the whole view.
 */
export function playerName(view: Pick<GameView, 'player_names'>, id: PlayerId): string {
  const name = view.player_names[id];
  return name !== undefined && name.length > 0 ? name : id;
}

/**
 * The roster display name for a lobby {@link SeatView}: the occupant's chosen name,
 * else a seat-derived label (`"Player N"`, 1-based from the seat's own index — a
 * real field, never parsed from the id). Callers add any "(you)" suffix themselves.
 */
export function seatDisplayName(seat: SeatView): string {
  if (seat.name !== undefined && seat.name.length > 0) return seat.name;
  return `Player ${seat.seat + 1}`;
}
