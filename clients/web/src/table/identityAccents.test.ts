/**
 * Per-player identity accents (§Identity): deterministic from the view's seat
 * order, so every client — receiver, opponent, spectator, fresh mount — derives
 * the same color for the same player with no client state.
 */
import { describe, expect, it } from 'vitest';
import { accentAlpha, IDENTITY_ACCENTS, identityAccent } from './identityAccents';

const seated = (seat_order: string[], you = 'p1') => ({
  seat_order,
  you,
  opponents: seat_order.filter((id) => id !== you).map((player_id) => ({ player_id })),
});

describe('identityAccent', () => {
  it('assigns accents by seat order, identically for every viewer', () => {
    const order = ['p1', 'p2', 'p3', 'p4'];
    // The receiver's perspective and an opponent's perspective agree.
    const mine = seated(order, 'p1');
    const theirs = seated(order, 'p3');
    for (const id of order) {
      expect(identityAccent(mine, id)).toBe(identityAccent(theirs, id));
    }
    expect(identityAccent(mine, 'p1')).toBe(IDENTITY_ACCENTS[0]);
    expect(identityAccent(mine, 'p4')).toBe(IDENTITY_ACCENTS[3]);
  });

  it('gives distinct accents to a full 4-player table', () => {
    const view = seated(['p1', 'p2', 'p3', 'p4']);
    const accents = ['p1', 'p2', 'p3', 'p4'].map((id) => identityAccent(view, id));
    expect(new Set(accents).size).toBe(4);
  });

  it('falls back to receiver-then-opponents order without seat_order', () => {
    const view = { seat_order: [], you: 'p1', opponents: [{ player_id: 'p2' }] };
    expect(identityAccent(view, 'p1')).toBe(IDENTITY_ACCENTS[0]);
    expect(identityAccent(view, 'p2')).toBe(IDENTITY_ACCENTS[1]);
  });

  it('never crashes on an unknown id (presentation only)', () => {
    const view = seated(['p1', 'p2']);
    expect(identityAccent(view, 'ghost')).toBe(IDENTITY_ACCENTS[0]);
  });
});

describe('accentAlpha', () => {
  it('appends a clamped two-digit hex alpha', () => {
    expect(accentAlpha('#3E9C9C', 255)).toBe('#3E9C9Cff');
    expect(accentAlpha('#3E9C9C', 0)).toBe('#3E9C9C00');
    expect(accentAlpha('#3E9C9C', 89)).toBe('#3E9C9C59');
  });
});
