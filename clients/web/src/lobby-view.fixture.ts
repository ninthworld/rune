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
  directory: [],
  valid_commands: ['create_room', 'join_room'],
};

/**
 * Room-less, but the directory (issue #280) now carries two rooms to browse: an
 * open `gathering` room with one of two seats filled, and a full `in_progress`
 * room. Written as raw wire JSON with the same eliding the server uses.
 */
export const LOBBY_DIRECTORY_JSON = JSON.stringify({
  session: 's:ab12',
  you: 'p1',
  directory: [
    {
      room_id: 'r0',
      config: { seats: 2, game_setup: '1v1' },
      filled: 1,
      state: 'gathering',
    },
    {
      room_id: 'r1',
      config: { seats: 4, game_setup: 'ffa-4' },
      filled: 4,
      state: 'in_progress',
    },
  ],
  valid_commands: ['create_room', 'join_room'],
});

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

/**
 * Host view of a room with an open seat (issue #415): you (p1) are the host at seat 0,
 * so the server offers `add_ai` alongside submit/leave. Seat 1 is open, awaiting a human
 * or an AI opponent.
 */
export const LOBBY_ROOM_HOST_CAN_ADD_AI_JSON = JSON.stringify({
  session: 's:ab12',
  you: 'p1',
  room: {
    room_id: 'r:7f3',
    config: { seats: 2, game_setup: '1v1' },
    seats: [{ seat: 0, occupied_by: 'p1', decked: true }, { seat: 1 }],
  },
  valid_commands: ['submit_deck', 'ready', 'add_ai', 'leave'],
});

/**
 * Host view once seat 1 is filled by a `random` AI opponent (issue #415): the AI seat
 * carries `ai`, no `occupied_by`, and is decked + ready by construction. The server now
 * offers `remove_ai` (and no longer `add_ai`, the room being full).
 */
export const LOBBY_ROOM_WITH_AI_JSON = JSON.stringify({
  session: 's:ab12',
  you: 'p1',
  room: {
    room_id: 'r:7f3',
    config: { seats: 2, game_setup: '1v1' },
    seats: [
      { seat: 0, occupied_by: 'p1', decked: true },
      { seat: 1, name: 'Random', decked: true, ready: true, ai: 'random' },
    ],
  },
  valid_commands: ['submit_deck', 'ready', 'remove_ai', 'leave'],
});

/**
 * A commander-format room (issue #372): you (p1) hold seat 0, undecked; you may
 * submit a deck or leave. The `game_setup` is `commander`, which is what gates the
 * client into designating and sending a deck's commander.
 */
export const LOBBY_ROOM_COMMANDER_JSON = JSON.stringify({
  session: 's:ab12',
  you: 'p1',
  room: {
    room_id: 'r:cmd',
    config: { seats: 4, game_setup: 'commander' },
    seats: [{ seat: 0, occupied_by: 'p1' }, { seat: 1 }, { seat: 2 }, { seat: 3 }],
  },
  valid_commands: ['submit_deck', 'leave'],
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
