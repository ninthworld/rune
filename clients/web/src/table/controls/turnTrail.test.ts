/**
 * The turn trail (issue #455): the path the current turn has already taken,
 * derived from ONE view's own log window.
 *
 * The properties that matter are as much about what it refuses to say as what it
 * says — it must not invent a path the server never recorded, must not carry a
 * previous turn's steps into this one, and must never claim the seat was
 * *skipped* anywhere. That last is a different fact with its own wire field
 * (`auto_passed_steps`, issue #455) drawn by its own mark on the plaque; the two
 * are not derivable from each other, and conflating them would announce a skip
 * that never happened.
 */
import { describe, expect, it } from 'vitest';
import type { GameLogEntry, GameView, Phase, PlayerId } from '../../protocol';
import { turnTrail } from './turnTrail';

function step(sequence: number, turn: number, phase: Phase, active: PlayerId = 'p1'): GameLogEntry {
  return { sequence, event: { type: 'step_changed', turn, active_player: active, phase } };
}

function viewWith(overrides: Partial<GameView> = {}): GameView {
  return {
    you: 'p1',
    my_hand: [],
    me: { life: 20, library_size: 40 },
    opponents: [],
    battlefield: [],
    stack: [],
    graveyards: [],
    exile: [],
    phase: 'precombat_main',
    turn: 2,
    active_player: 'p1',
    seat_order: ['p1', 'p2'],
    mana_pool: [],
    valid_actions: [],
    player_names: {},
    commander_damage: [],
    ...overrides,
  };
}

describe('turnTrail', () => {
  it('lists the steps this turn passed through, excluding the current one', () => {
    const view = viewWith({
      turn: 2,
      phase: 'precombat_main',
      log: [
        step(1, 2, 'untap'),
        step(2, 2, 'upkeep'),
        step(3, 2, 'draw'),
        step(4, 2, 'precombat_main'),
      ],
    });
    expect(turnTrail(view)).toEqual(['untap', 'upkeep', 'draw']);
  });

  it('drops an earlier turn’s steps rather than drawing one path across two turns', () => {
    // The exact shape ADR 0020's settle loop produces: a whole opponent turn and
    // the start of the next one arriving in a single broadcast.
    const view = viewWith({
      turn: 3,
      phase: 'upkeep',
      log: [
        step(1, 2, 'postcombat_main', 'p2'),
        step(2, 2, 'end', 'p2'),
        step(3, 2, 'cleanup', 'p2'),
        step(4, 3, 'untap'),
        step(5, 3, 'upkeep'),
      ],
    });
    expect(turnTrail(view)).toEqual(['untap']);
  });

  it('collapses a repeated step rather than listing it twice', () => {
    const view = viewWith({
      turn: 2,
      phase: 'draw',
      log: [step(1, 2, 'untap'), step(2, 2, 'untap'), step(3, 2, 'draw')],
    });
    expect(turnTrail(view)).toEqual(['untap']);
  });

  it('is empty when the log window carries nothing for this turn', () => {
    // A reconnect whose window opened mid-turn, or a server that trimmed it: the
    // honest answer is "no path I can see", never a guessed one.
    const view = viewWith({ turn: 5, phase: 'end', log: [step(1, 4, 'cleanup', 'p2')] });
    expect(turnTrail(view)).toEqual([]);
  });

  it('is empty with no log at all (the optional-field convention)', () => {
    expect(turnTrail(viewWith({ log: undefined }))).toEqual([]);
  });

  it('reads only `step_changed` — no other event contributes a step', () => {
    const view = viewWith({
      turn: 2,
      phase: 'draw',
      log: [
        step(1, 2, 'untap'),
        { sequence: 2, event: { type: 'cards_drawn', player: 'p1', count: 1 } },
        step(3, 2, 'draw'),
      ],
    });
    expect(turnTrail(view)).toEqual(['untap']);
  });
});
