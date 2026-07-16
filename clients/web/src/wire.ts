/**
 * Wire parsing for server→client messages.
 *
 * The only server→client message is a {@link GameView} (docs/protocol.md). The
 * server omits empty collections and absent optionals, so this module normalizes
 * a parsed payload into a fully-populated `GameView` where every required
 * collection is present. This is wire hygiene, not game logic: no legality,
 * cost, or effect is ever computed here — unknown fields are tolerated for
 * forward compatibility.
 */
import {
  type GameResult,
  type GameView,
  type LobbyView,
  PHASES,
  type Phase,
  type PlayerId,
  type RoomConfig,
  type RoomView,
  type SeatView,
  type SelfView,
} from './protocol';

/** Raised when a server payload is not a decodable {@link GameView}. */
export class ProtocolError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'ProtocolError';
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function isPhase(value: unknown): value is Phase {
  return typeof value === 'string' && (PHASES as readonly string[]).includes(value);
}

/**
 * Coerce a wire value into an array, treating an omitted field (`undefined`) as
 * the documented empty default. A present-but-non-array value is a protocol
 * violation and throws.
 */
function asArray<T>(value: unknown, field: string): T[] {
  if (value === undefined) return [];
  if (!Array.isArray(value)) {
    throw new ProtocolError(`GameView.${field} must be an array`);
  }
  return value as T[];
}

/**
 * Normalize the receiver's own {@link SelfView} stats. An older server may omit the
 * whole object, or a field; each missing value defaults to `0`, so the client always
 * has a number to display and never invents anything the server did not send.
 */
function normalizeSelfView(payload: unknown): SelfView {
  if (!isRecord(payload)) return { life: 0, library_size: 0 };
  return {
    life: typeof payload.life === 'number' ? payload.life : 0,
    library_size: typeof payload.library_size === 'number' ? payload.library_size : 0,
  };
}

/**
 * Normalize the optional terminal {@link GameResult} half of a {@link GameView}.
 * Returns `undefined` while the game is live (the server omits `result`, or sends
 * a malformed object with no string `reason`), so its mere presence signals game
 * over. `losers` defaults to the empty array; `winner` stays absent for a draw.
 * The `reason` is carried through verbatim — the client renders it and derives no
 * terminality of its own; an unrecognized future value is tolerated (forward
 * compatibility) and handled generically by the game-over overlay.
 */
function normalizeGameResult(payload: unknown): GameResult | undefined {
  if (!isRecord(payload) || typeof payload.reason !== 'string') return undefined;
  const result: GameResult = {
    losers: asArray<PlayerId>(payload.losers, 'result.losers'),
    reason: payload.reason as GameResult['reason'],
  };
  if (typeof payload.winner === 'string') result.winner = payload.winner;
  return result;
}

/**
 * Normalize an already-parsed payload into a complete {@link GameView}. Missing
 * collections become empty arrays; optional scalars (`priority_player`,
 * `action_deadline`) are carried through untouched, and the terminal `result` is
 * carried through only when the game is over. Throws {@link ProtocolError} if the
 * payload is not an object or lacks a valid `phase`.
 */
export function normalizeGameView(payload: unknown): GameView {
  if (!isRecord(payload)) {
    throw new ProtocolError('GameView payload must be a JSON object');
  }
  if (!isPhase(payload.phase)) {
    throw new ProtocolError(`GameView.phase is missing or invalid: ${String(payload.phase)}`);
  }

  return {
    // An older server may omit `you`; default to '' so the payload still
    // normalizes rather than crashing (forward/backward compatibility).
    you: typeof payload.you === 'string' ? payload.you : '',
    my_hand: asArray(payload.my_hand, 'my_hand'),
    me: normalizeSelfView(payload.me),
    opponents: asArray(payload.opponents, 'opponents'),
    battlefield: asArray(payload.battlefield, 'battlefield'),
    stack: asArray(payload.stack, 'stack'),
    graveyards: asArray(payload.graveyards, 'graveyards'),
    exile: asArray(payload.exile, 'exile'),
    phase: payload.phase,
    mana_pool: asArray(payload.mana_pool, 'mana_pool'),
    priority_player:
      typeof payload.priority_player === 'string' ? payload.priority_player : undefined,
    valid_actions: asArray(payload.valid_actions, 'valid_actions'),
    action_deadline:
      typeof payload.action_deadline === 'number' ? payload.action_deadline : undefined,
    result: normalizeGameResult(payload.result),
  };
}

/**
 * Parse a raw server→client text frame into a {@link GameView}. Throws
 * {@link ProtocolError} on malformed JSON or an invalid shape.
 */
export function parseGameView(raw: string): GameView {
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch (cause) {
    throw new ProtocolError(`server frame is not valid JSON: ${String(cause)}`);
  }
  return normalizeGameView(parsed);
}

/** Coerce a wire value into a string, treating an omitted field as `''`. */
function asString(value: unknown): string {
  return typeof value === 'string' ? value : '';
}

/** Normalize a wire `RoomConfig`, defaulting a missing/invalid `seats` to `0`. */
function normalizeRoomConfig(payload: unknown): RoomConfig {
  const record = isRecord(payload) ? payload : {};
  return {
    seats: typeof record.seats === 'number' ? record.seats : 0,
    game_setup: asString(record.game_setup),
  };
}

/**
 * Normalize a wire {@link SeatView}. `occupied_by` stays absent for an empty
 * seat; `decked`/`ready` default to `false` (the server elides them when false).
 */
function normalizeSeatView(payload: unknown, index: number): SeatView {
  const record = isRecord(payload) ? payload : {};
  const seat: SeatView = {
    seat: typeof record.seat === 'number' ? record.seat : index,
    decked: record.decked === true,
    ready: record.ready === true,
  };
  if (typeof record.occupied_by === 'string') seat.occupied_by = record.occupied_by;
  return seat;
}

/** Normalize the optional room half of a {@link LobbyView}. */
function normalizeRoomView(payload: unknown): RoomView {
  const record = isRecord(payload) ? payload : {};
  return {
    room_id: asString(record.room_id),
    config: normalizeRoomConfig(record.config),
    seats: asArray(record.seats, 'room.seats').map(normalizeSeatView),
  };
}

/**
 * Normalize an already-parsed payload into a complete {@link LobbyView}. Missing
 * `session`/`you` default to `''` (like `GameView.you`), `room` stays absent when
 * omitted, and `valid_commands` becomes the empty array. This is wire hygiene, not
 * game logic — unknown fields are tolerated for forward compatibility.
 */
export function normalizeLobbyView(payload: unknown): LobbyView {
  if (!isRecord(payload)) {
    throw new ProtocolError('LobbyView payload must be a JSON object');
  }
  const view: LobbyView = {
    session: asString(payload.session),
    you: asString(payload.you),
    valid_commands: asArray<string>(payload.valid_commands, 'valid_commands'),
  };
  if (isRecord(payload.room)) view.room = normalizeRoomView(payload.room);
  return view;
}

/**
 * One decoded server→client frame: either an in-game {@link GameView} or a
 * pre-game {@link LobbyView}. The two are distinguished structurally — a
 * `GameView` always carries a valid {@link Phase}; a `LobbyView` never does — so
 * a single connection can carry both across the lobby→game handoff.
 */
export type ServerFrame =
  | { readonly kind: 'game'; readonly view: GameView }
  | { readonly kind: 'lobby'; readonly lobby: LobbyView };

/**
 * Parse a raw server→client text frame, routing it to a {@link GameView} or a
 * {@link LobbyView} by the presence of a valid `phase`. Throws
 * {@link ProtocolError} on malformed JSON or a non-object payload.
 */
export function parseServerFrame(raw: string): ServerFrame {
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch (cause) {
    throw new ProtocolError(`server frame is not valid JSON: ${String(cause)}`);
  }
  if (isRecord(parsed) && isPhase(parsed.phase)) {
    return { kind: 'game', view: normalizeGameView(parsed) };
  }
  return { kind: 'lobby', lobby: normalizeLobbyView(parsed) };
}
