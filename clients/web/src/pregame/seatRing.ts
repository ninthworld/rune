/**
 * Where a seat sits in the ready room's arena (issue #546).
 *
 * The approved game-lobby baseline (`docs/ui-concepts/rune-pregame-game-lobby.jpg`)
 * makes the arena itself the seating diagram: seats stand around the ring the
 * environment already draws, with **the local seat at the bottom** — the same
 * place that player's own hand will be a moment later — and the others spread
 * around it in seat order. That single rule is what makes the room read as the
 * table you are about to play at rather than as a roster of rows.
 *
 * This module is the whole of that rule, as a pure function over the seat count
 * and which seat is yours. It computes **no legality and no game state**: seat
 * count comes from the room's configuration and the occupants come from the
 * view; this only decides where on the ellipse each one is drawn.
 */

/** The ring's horizontal radius, as a percentage of the arena's width. */
export const RING_RX = 36;

/** The ring's vertical radius, as a percentage of the arena's height. */
export const RING_RY = 34;

/** One seat's drawn position, as CSS percentages of the arena box. */
export interface SeatSlot {
  /** The room seat index this slot draws. */
  readonly seat: number;
  /** Horizontal centre, e.g. `"50%"`. */
  readonly x: string;
  /** Vertical centre, e.g. `"84%"`. */
  readonly y: string;
  /** Whether this is the local player's seat (the bottom anchor). */
  readonly local: boolean;
}

/**
 * Place `total` seats around the ring, anchoring `localSeat` at the bottom.
 *
 * The k-th seat clockwise from the anchor sits at `90° + k · 360/total`, measured
 * in the screen's own coordinates (y grows downward), so k = 0 lands at bottom
 * centre and the rest run left → top → right. At four seats that is exactly the
 * baseline's arrangement: you at the bottom, one to the left, one at the top, an
 * open seat to the right.
 *
 * `localSeat` may be absent — a view can name a room before it names your seat —
 * in which case seat 0 takes the anchor and no slot is marked local. Seats are
 * returned in room-seat order so the caller's list order never depends on this.
 */
export function seatRing(total: number, localSeat?: number): SeatSlot[] {
  const count = Math.max(0, Math.floor(total));
  if (count === 0) return [];
  const anchor = localSeat !== undefined && localSeat >= 0 && localSeat < count ? localSeat : 0;

  return Array.from({ length: count }, (_, seat) => {
    const step = (seat - anchor + count) % count;
    const angle = Math.PI / 2 + (2 * Math.PI * step) / count;
    return {
      seat,
      x: `${round(50 + RING_RX * Math.cos(angle))}%`,
      y: `${round(50 + RING_RY * Math.sin(angle))}%`,
      local: localSeat !== undefined && seat === localSeat,
    };
  });
}

/** Two decimals — enough for a CSS percentage, short enough to read in a test. */
function round(value: number): number {
  return Math.round(value * 100) / 100;
}
