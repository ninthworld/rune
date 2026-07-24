/**
 * Seat identity for the pregame roster (issue #506;
 * `docs/design/front-door-and-lobby.md` §4.3, §5.3, §5.9).
 *
 * Identity is taught **once**: pregame and match read `SCENE_SEAT_ACCENTS` from
 * the same index, so a seat's color never changes as the game starts. The room
 * indexes by `SeatView.seat`; the match indexes by the position of the player id
 * in `GameView.seat_order`, and the server builds `seat_order` in room-seat order
 * (`crates/rune-server/src/view.rs`), so the two agree — pinned by a test rather
 * than asserted in prose.
 *
 * Color is never the only carrier: the crest chip's monogram, the display name,
 * the roster row's position, and the "You" tag all say the same thing (§5.9).
 *
 * Pure presentation reads of the view — no game logic, no I/O.
 */
import type { SeatView } from '../protocol';
import { seatDisplayName } from '../playerNames';

/**
 * The crest chip's monogram: the first glyph of the seat's display name,
 * uppercased. `seatDisplayName` already falls back to the seat-derived
 * `Player N` label, so an unnamed seat gets a stable glyph from its own seat
 * number rather than from a parsed player id.
 *
 * Uses the code-point iterator so an astral-plane first character (an emoji, a
 * non-BMP script) is not split into a lone surrogate.
 */
export function seatMonogram(seat: SeatView): string {
  const name = seatDisplayName(seat).trim();
  const first = [...name][0];
  return first === undefined ? String(seat.seat + 1) : first.toUpperCase();
}

/**
 * Whether a seat counts as filled for presentation. An AI seat (issue #415) has
 * no `occupied_by` but is still a filled seat.
 */
export function seatFilled(seat: SeatView): boolean {
  return seat.occupied_by !== undefined || seat.ai !== undefined;
}
