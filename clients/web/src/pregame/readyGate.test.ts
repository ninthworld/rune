/**
 * The ready gate's five states (issue #506; `front-door-and-lobby.md` §8
 * criterion 9): no deck submitted; deck submitted and not ready; waiting for
 * players; ready and waiting; starting.
 *
 * Every expectation here is a read of one `LobbyView` — the gate never computes
 * legality, and `gold` only ever names a command the view advertised.
 */
import { describe, expect, it } from 'vitest';
import type { LobbyView, SeatView } from '../protocol';
import { readyGate, waitingForNames } from './readyGate';

/** Build a room view: seats plus the commands the server advertised. */
function room(seats: SeatView[], validCommands: string[], you = 'p1'): LobbyView {
  return {
    session: 's:1',
    you,
    directory: [],
    room: {
      room_id: 'r:1',
      config: { seats: seats.length, game_setup: '1v1' },
      seats,
    },
    valid_commands: validCommands,
  } as LobbyView;
}

describe('readyGate — the gate in words', () => {
  it('has no gate outside a room', () => {
    const roomless = {
      session: 's:1',
      you: 'p1',
      directory: [],
      valid_commands: ['create_room'],
    } as unknown as LobbyView;
    expect(readyGate(roomless)).toBeNull();
  });

  it('state 1 — no deck submitted: the gold control is Submit deck', () => {
    const gate = readyGate(
      room([{ seat: 0, occupied_by: 'p1' }, { seat: 1 }], ['submit_deck', 'leave']),
    );
    expect(gate?.sentence).toBe('Choose and submit a deck');
    expect(gate?.gold).toBe('submit_deck');
    expect(gate?.unready).toBe(false);
  });

  it('state 2 — waiting for players: the sentence counts the open seats', () => {
    const gate = readyGate(
      room(
        [{ seat: 0, occupied_by: 'p1', decked: true }, { seat: 1 }, { seat: 2 }],
        ['submit_deck', 'ready', 'leave'],
      ),
    );
    expect(gate?.sentence).toBe('Waiting for 2 more players to join');
    expect(gate?.openSeats).toBe(2);
    // Ready is still advertised, so it is still the next step you can take.
    expect(gate?.gold).toBe('ready');
  });

  it('pluralizes a single missing player', () => {
    const gate = readyGate(
      room(
        [{ seat: 0, occupied_by: 'p1', decked: true }, { seat: 1 }],
        ['submit_deck', 'ready', 'leave'],
      ),
    );
    expect(gate?.sentence).toBe('Waiting for 1 more player to join');
  });

  it('state 3 — deck submitted, room full, not ready: ready up to start', () => {
    const gate = readyGate(
      room(
        [
          { seat: 0, occupied_by: 'p1', decked: true },
          { seat: 1, occupied_by: 'p2', decked: true },
        ],
        ['submit_deck', 'ready', 'leave'],
      ),
    );
    expect(gate?.sentence).toBe("Everyone's here — ready up to start");
    expect(gate?.gold).toBe('ready');
  });

  it('state 4 — ready and waiting: the sentence names who the room waits on', () => {
    const gate = readyGate(
      room(
        [
          { seat: 0, occupied_by: 'p1', decked: true, ready: true },
          { seat: 1, occupied_by: 'p2', name: 'Bob', decked: true },
        ],
        ['unready', 'leave'],
      ),
    );
    expect(gate?.sentence).toBe("You're ready — waiting for Bob");
    // No gold: there is nothing left for you to do.
    expect(gate?.gold).toBeNull();
    expect(gate?.unready).toBe(true);
    expect(gate?.ready).toBe(true);
  });

  it('state 4b — ready with seats still open names the seats, not a person', () => {
    const gate = readyGate(
      room(
        [{ seat: 0, occupied_by: 'p1', decked: true, ready: true }, { seat: 1 }],
        ['unready', 'leave'],
      ),
    );
    expect(gate?.sentence).toBe("You're ready — waiting for 1 more player to join");
  });

  it('state 5 — starting: every seat filled, decked, and ready', () => {
    const gate = readyGate(
      room(
        [
          { seat: 0, occupied_by: 'p1', decked: true, ready: true },
          { seat: 1, occupied_by: 'p2', decked: true, ready: true },
        ],
        ['unready', 'leave'],
      ),
    );
    expect(gate?.starting).toBe(true);
    expect(gate?.sentence).toBe('Starting the game…');
    // Nothing to advance: the server is constructing the game on this frame.
    expect(gate?.gold).toBeNull();
  });

  it('counts an AI seat as filled, decked, and ready (issue #415)', () => {
    const gate = readyGate(
      room(
        [
          { seat: 0, occupied_by: 'p1', decked: true, ready: true },
          { seat: 1, name: 'Random', ai: 'random', decked: true, ready: true },
        ],
        ['unready', 'leave'],
      ),
    );
    expect(gate?.starting).toBe(true);
    expect(gate?.openSeats).toBe(0);
  });

  it('offers no gold for a command the server did not advertise', () => {
    // `ready` absent from valid_commands: the bar states the gate and offers
    // nothing, because the client computes no legality of its own.
    const gate = readyGate(
      room(
        [
          { seat: 0, occupied_by: 'p1', decked: true },
          { seat: 1, occupied_by: 'p2', decked: true },
        ],
        ['leave'],
      ),
    );
    expect(gate?.gold).toBeNull();
    expect(gate?.sentence).toBe('Waiting for the other players');
  });
});

describe('waitingForNames — the bar never grows a paragraph', () => {
  it('reads one, two, and many names', () => {
    expect(waitingForNames([])).toBe('');
    expect(waitingForNames(['Bob'])).toBe('Bob');
    expect(waitingForNames(['Bob', 'Ann'])).toBe('Bob and Ann');
    expect(waitingForNames(['Bob', 'Ann', 'Cid'])).toBe('Bob, Ann and 1 other');
    expect(waitingForNames(['Bob', 'Ann', 'Cid', 'Dot', 'Eve'])).toBe('Bob, Ann and 3 others');
  });
});
