/**
 * Representative server→client `LobbyView` frames for the lobby suite (issue
 * #114), mirroring the round-trip fixtures in `crates/rune-protocol/src/lib.rs`.
 * Written as raw wire JSON — empty collections and false flags are elided exactly
 * as the server elides them, so tests exercise the client's normalization.
 *
 * Shared by the unit suites so the client and the server cannot silently disagree
 * about the lobby wire shape.
 */
import type { LobbyView } from './protocol';

/** The first lobby frame after `Hello`: not in a room; can create or join. */
export const LOBBY_ROOMLESS_JSON = JSON.stringify({
  session: 's:ab12',
  you: 'p1',
  valid_commands: ['create_room', 'join_room'],
});

/** The typed form of {@link LOBBY_ROOMLESS_JSON} (post-normalization). */
export const LOBBY_ROOMLESS: LobbyView = {
  session: 's:ab12',
  you: 'p1',
  valid_commands: ['create_room', 'join_room'],
};

/**
 * In a freshly created 2-seat room: you (p1) hold seat 0, undecked; seat 1 is
 * open. You may submit a deck or leave. Your `game_setup` is opaque here.
 */
export const LOBBY_ROOM_UNDECKED_JSON = JSON.stringify({
  session: 's:ab12',
  you: 'p1',
  room: {
    room_id: 'r:7f3',
    config: { seats: 2, game_setup: '1v1' },
    seats: [{ seat: 0, occupied_by: 'p1' }, { seat: 1 }],
  },
  valid_commands: ['submit_deck', 'leave'],
});

/** After submitting a deck: seat 0 is decked; you may ready up now. */
export const LOBBY_ROOM_DECKED_JSON = JSON.stringify({
  session: 's:ab12',
  you: 'p1',
  room: {
    room_id: 'r:7f3',
    config: { seats: 2, game_setup: '1v1' },
    seats: [{ seat: 0, occupied_by: 'p1', decked: true }, { seat: 1 }],
  },
  valid_commands: ['submit_deck', 'ready', 'leave'],
});

/** After readying: seat 0 is decked + ready (still waiting on the other seat). */
export const LOBBY_ROOM_READY_JSON = JSON.stringify({
  session: 's:ab12',
  you: 'p1',
  room: {
    room_id: 'r:7f3',
    config: { seats: 2, game_setup: '1v1' },
    seats: [{ seat: 0, occupied_by: 'p1', decked: true, ready: true }, { seat: 1 }],
  },
  valid_commands: ['submit_deck', 'unready', 'leave'],
});

/** A full room where both seats are filled, decked, and ready. */
export const LOBBY_ROOM_ALL_READY_JSON = JSON.stringify({
  session: 's:ab12',
  you: 'p1',
  room: {
    room_id: 'r:7f3',
    config: { seats: 2, game_setup: '1v1' },
    seats: [
      { seat: 0, occupied_by: 'p1', decked: true, ready: true },
      { seat: 1, occupied_by: 'p2', decked: true, ready: true },
    ],
  },
  valid_commands: ['unready', 'leave'],
});
