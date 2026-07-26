/**
 * Gates for the ready room's seating geometry (issue #546).
 *
 * The rule the approved game-lobby baseline sets is small and easy to break by
 * accident: **your seat is at the bottom, and the others run left → top → right
 * from it, in seat order.** These pin exactly that, plus the two degenerate
 * inputs a real `LobbyView` can produce.
 */
import { describe, expect, it } from 'vitest';
import { RING_RX, RING_RY, seatRing } from './seatRing';

describe('seatRing — the local seat anchors the bottom', () => {
  it('lays four seats out as the baseline draws them: you, left, top, right', () => {
    const slots = seatRing(4, 0);
    expect(slots.map((slot) => `${slot.x} ${slot.y}`)).toEqual([
      `50% ${50 + RING_RY}%`,
      `${50 - RING_RX}% 50%`,
      `50% ${50 - RING_RY}%`,
      `${50 + RING_RX}% 50%`,
    ]);
    expect(slots.map((slot) => slot.local)).toEqual([true, false, false, false]);
  });

  it('rotates the ring so any seat can be the local one, in seat order', () => {
    // Seat 2 is mine: it takes the bottom, and 3, 0, 1 follow it round. The
    // returned order stays room-seat order, so a caller never depends on this.
    const slots = seatRing(4, 2);
    expect(slots.map((slot) => slot.seat)).toEqual([0, 1, 2, 3]);
    expect(slots[2]).toEqual({ seat: 2, x: '50%', y: `${50 + RING_RY}%`, local: true });
    expect(slots[3]!.x).toBe(`${50 - RING_RX}%`);
    expect(slots[0]!.y).toBe(`${50 - RING_RY}%`);
    expect(slots[1]!.x).toBe(`${50 + RING_RX}%`);
  });

  it('seats a duel head to head', () => {
    const slots = seatRing(2, 1);
    expect(slots[1]!.y).toBe(`${50 + RING_RY}%`);
    expect(slots[0]!.y).toBe(`${50 - RING_RY}%`);
  });

  it('anchors seat 0 when the view names a room before it names your seat', () => {
    // A `LobbyView` can carry a room this connection holds no seat in; the ring
    // still has to draw, and nothing may claim to be local.
    const slots = seatRing(3);
    expect(slots[0]!.y).toBe(`${50 + RING_RY}%`);
    expect(slots.every((slot) => !slot.local)).toBe(true);
  });

  it('ignores a local seat outside the room and never returns a broken slot', () => {
    const slots = seatRing(2, 7);
    expect(slots).toHaveLength(2);
    // Seat 0 takes the anchor; nothing is marked local, because seat 7 is not here.
    expect(slots[0]!.y).toBe(`${50 + RING_RY}%`);
    expect(slots.every((slot) => !slot.local)).toBe(true);
    expect(seatRing(0)).toEqual([]);
    expect(seatRing(-3)).toEqual([]);
  });

  it('spaces every seat evenly, at every supported room size', () => {
    // The protocol's own range is 2..=8; each size must produce that many
    // distinct positions, so two seats can never be drawn on top of each other.
    for (let total = 2; total <= 8; total += 1) {
      const slots = seatRing(total, 0);
      expect(slots).toHaveLength(total);
      expect(new Set(slots.map((slot) => `${slot.x}|${slot.y}`)).size).toBe(total);
    }
  });
});
