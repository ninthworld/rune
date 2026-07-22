/**
 * Lobby protocol: pre-game state, rooms, commands, and deck submission.
 */

import type { PlayerId } from './index.js';

/**
 * Server-issued opaque session/reconnect token. The client stores it and echoes
 * it verbatim on a later {@link HelloCommand}} (after a refresh or dropped socket).
 * It is an identity handle only — never parsed, never authentication of a human.
 */
export type SessionToken = string;

/** Opaque room id, issued on {@link CreateRoomCommand}} and shared out-of-band. */
export type RoomId = string;

/** Opaque game-setup id carried in a {@link RoomConfig}}; the server validates it. */
export type GameSetupId = string;

/**
 * Opaque card-identity handle in a submitted decklist. These are card
 * *identities*, never printings or images (project legal rules); the server
 * validates each against its card database and the client never parses them.
 */
export type CardIdentity = string;

/** A room's configuration, supplied by the creator and echoed in every view. */
export interface RoomConfig {
  /** Number of seats, validated server-side into the inclusive range `2..=8`. */
  seats: number;
  /** Opaque game-setup id naming which setup the room builds its game from. */
  game_setup: GameSetupId;
}

/**
 * One seat in a room's roster, as any connection sees it. Hidden information
 * stays redacted: a seat's decklist contents are never exposed, only that the
 * seat is decked.
 */
export interface SeatView {
  /** Zero-based seat index within the room. */
  seat: number;
  /** The player occupying this seat; absent when the seat is empty. */
  occupied_by?: PlayerId;
  /**
   * The occupant's chosen display name (issue #294), if set — public, display-only
   * text. The seat's identity is still its {@link SeatView.occupied_by}} id. Absent for
   * an empty or unnamed seat, in which case the client falls back to a seat-derived
   * label (e.g. `"Player 2"`), so an older server that omits names keeps working.
   */
  name?: string;
  /** Whether this seat has submitted a server-validated deck (defaults `false`). */
  decked?: boolean;
  /** Whether this seat has declared itself ready (defaults `false`). */
  ready?: boolean;
  /**
   * When this seat is filled by an AI opponent (issue #415), the id of the AI kind
   * occupying it (e.g. `"random"`) — one of the {@link CatalogView.ai_opponents}} ids.
   * Absent for an empty seat or a human occupant (identified by
   * {@link SeatView.occupied_by}} instead). An AI seat carries no `occupied_by` and always
   * reports `decked`/`ready` as `true`. A free-form string like the other id fields, so a
   * newer AI kind never breaks an older client; render the kind's label from the catalog.
   */
  ai?: string;
}

/** The room a connection is in, with its config and full seat roster. */
export interface RoomView {
  /** The room's opaque id, shared to invite a second player. */
  room_id: RoomId;
  /** The room's configuration. */
  config: RoomConfig;
  /** Every seat in the room, in seat order. */
  seats: SeatView[];
}

/**
 * A room's lifecycle state in the lobby {@link LobbyView.directory}} (issue #280):
 * `gathering` (pre-game, joinable while it has an open seat) or `in_progress` (its
 * game has started — visible for context but not joinable). A finished or emptied
 * room leaves the directory entirely. A client tolerates an unknown future value.
 */
export type RoomState = 'gathering' | 'in_progress';

/**
 * One room as it appears in the public room **directory** — enough to browse and
 * join an open game without an out-of-band id, and no more. It carries no seat
 * roster and no player-identifying info beyond the occupancy count, and never any
 * game state.
 */
export interface RoomSummary {
  /** The room's opaque id — the same id a `join_room` command carries. */
  room_id: RoomId;
  /** The room's configuration (seat count and game setup). */
  config: RoomConfig;
  /** How many of the room's seats are occupied; the total is `config.seats`. */
  filled: number;
  /**
   * How many observers are watching the room (issue #351). Spectators do not
   * consume seats, so this is independent of {@link RoomSummary.filled}}; a count
   * only, never a spectator's identity. Defaults to `0` when the server omits it.
   */
  spectators: number;
  /** The room's lifecycle state (`gathering` or `in_progress`). */
  state: RoomState;
}

/**
 * The full pre-game state for one connection, pushed on every change — the
 * pre-game analogue of {@link GameView}}. The client rebuilds its entire pre-game
 * UI from a single `LobbyView` (reconnect-safe by construction) and derives no
 * legality: {@link LobbyView.valid_commands}} is the only source of interactivity.
 */
export interface LobbyView {
  /**
   * The connection's session/reconnect token (private — the client's own handle,
   * distinct from the public {@link LobbyView.you}}). Always present on the wire;
   * defaulted to `''` if a payload omits it.
   */
  session: SessionToken;
  /**
   * The connection's public player identity, used to match itself against a
   * {@link SeatView.occupied_by}}. Defaults to `''` when absent.
   */
  you: PlayerId;
  /**
   * The connection's own chosen display name (issue #294), if set via a
   * {@link SetNameCommand}}. Lets the pre-game UI show the local player's name before a
   * seat exists; once seated, the same name also rides in the roster's
   * {@link SeatView.name}}. Absent when unset.
   */
  name?: string;
  /** The room the connection is in, if any. Absent when not in a room. */
  room?: RoomView;
  /**
   * The public room **directory** (issue #280): every browsable room in the lobby,
   * so a player can discover and join an open game without an out-of-band id. Each
   * entry is a {@link RoomSummary}} (id, config, occupancy, lifecycle state); no seat
   * roster or player-identifying info, and no game state. Pushed on every room
   * lifecycle change. Elided on the wire when empty; {@link normalizeLobbyView}}
   * defaults it to an empty array.
   */
  directory: RoomSummary[];
  /**
   * The lobby command kinds currently legal for this connection (e.g.
   * `"create_room"`, `"join_room"`, `"submit_deck"`, `"ready"`, `"unready"`,
   * `"leave"`). Free-form strings; the client renders exactly these and tolerates
   * unknown kinds, computing no legality of its own.
   */
  valid_commands: string[];
}

/**
 * A structured, human-readable explanation of why a lobby command was rejected
 * (issue #395), pushed to the rejecting connection only. The primary case is a
 * rejected `submit_deck`: {@link LobbyRejection.reason}} is the server's own
 * explanation (safe to display verbatim — the client composes no prose of its own),
 * {@link LobbyRejection.code}} is a stable class id, and {@link LobbyRejection.card}}
 * names the offending card by its identity when one specific card is at fault. The
 * named card is always from the sender's own submission — never another seat's deck.
 */
export interface LobbyRejection {
  /**
   * Stable machine code for the rejection class, e.g. `"below_minimum"`,
   * `"above_maximum"`, `"copy_limit"`, `"missing_commander"`,
   * `"commander_not_in_deck"`, `"commander_not_legendary_creature"`,
   * `"out_of_identity"`, or `"unknown_card"`. Free-form so a newer server can add a
   * class without breaking an older client, which falls back to {@link LobbyRejection.reason}}.
   */
  code: string;
  /** Human-readable reason, safe to display verbatim. */
  reason: string;
  /**
   * The offending card's identity (`functional_id`), present only when the rejection
   * is about one specific card (a copy-limit or color-identity violation, or an
   * illegal/absent commander designation). Absent otherwise.
   */
  card?: CardIdentity;
}

/**
 * The server→client frame carrying a {@link LobbyRejection}} to the connection whose
 * command was rejected (issue #395). Its single `lobby_error` key distinguishes it on
 * the wire from every other frame; an older client that ignores it simply keeps its
 * current {@link LobbyView}}, so the feedback is additive.
 */
export interface LobbyErrorFrame {
  /** The structured rejection reason for the receiving connection. */
  lobby_error: LobbyRejection;
}

/** First-contact / reconnect command; carries a prior token when reconnecting. */
export interface HelloCommand {
  /** Discriminator. */
  type: 'hello';
  /** A previously issued session token to reclaim a held-open seat (or omitted). */
  token?: SessionToken;
}

/** Create a new room with the given config; the reply carries the new room id. */
export interface CreateRoomCommand {
  /** Discriminator. */
  type: 'create_room';
  /** The configuration for the new room. */
  config: RoomConfig;
}

/** Join an existing room by id (no matchmaking or discovery). */
export interface JoinRoomCommand {
  /** Discriminator. */
  type: 'join_room';
  /** The opaque id of the room to join. */
  room_id: RoomId;
}

/**
 * Watch an in-progress room as a spectator (ADR 0022, issue #351): a non-seated
 * observer receiving redacted {@link SpectatorView}}s. Unlike `join_room` it consumes
 * no seat and succeeds on a full room, but the room's game must already be running.
 */
export interface SpectateRoomCommand {
  /** Discriminator. */
  type: 'spectate_room';
  /** The opaque id of the room to spectate. */
  room_id: RoomId;
}

/** Submit a decklist for this connection's seat (server-validated). */
export interface SubmitDeckCommand {
  /** Discriminator. */
  type: 'submit_deck';
  /** The card identities, duplicates repeated; omitted when empty. */
  cards?: CardIdentity[];
  /**
   * The card this seat designates as its commander (CR 903.3, issue #372), by the
   * same `CardIdentity` its decklist uses. Present only for a commander-format deck
   * and omitted otherwise, so the frame stays the pre-commander shape. The server
   * validates the designation authoritatively; the client never computes legality.
   */
  commander?: CardIdentity;
}

/**
 * Fill an empty seat with an AI opponent (issue #415). Host-only: the server accepts it
 * only from the seat 0 occupant, for an empty seat of the host's own pre-game room. It
 * names the target `seat`, the `kind` of AI (one of {@link CatalogView.ai_opponents}}), and
 * the deck the AI plays — the same flat identity list a {@link SubmitDeckCommand}} carries,
 * validated authoritatively against the room's format. The client renders the affordance
 * only when the server advertises `add_ai` in `valid_commands`; it never computes host-ness.
 */
export interface AddAiCommand {
  /** Discriminator. */
  type: 'add_ai';
  /** Zero-based index of the seat to fill with an AI opponent. */
  seat: number;
  /** The AI kind to seat, one of {@link CatalogView.ai_opponents}} ids (e.g. `"random"`). */
  kind: string;
  /** The AI's deck as card identities, duplicates repeated; omitted when empty. */
  cards?: CardIdentity[];
  /** The AI's designated commander for a commander-format room; omitted otherwise. */
  commander?: CardIdentity;
}

/**
 * Remove an AI opponent from a seat (issue #415), emptying it again. Host-only and
 * pre-game, the counterpart of {@link AddAiCommand}}.
 */
export interface RemoveAiCommand {
  /** Discriminator. */
  type: 'remove_ai';
  /** Zero-based index of the AI seat to empty. */
  seat: number;
}

/** Declare or retract readiness for this connection's seat. */
export interface ReadyCommand {
  /** Discriminator. */
  type: 'ready';
  /** `true` to ready up, `false` to un-ready. */
  ready: boolean;
}

/**
 * Set (or change) this connection's public display name (issue #294). The server
 * validates it (length bounds, printable characters) and rejects an invalid value
 * with the lobby's non-fatal error pattern (the current {@link LobbyView}} is
 * re-sent). The name is bound to the session, so it survives a per-tab reconnect.
 */
export interface SetNameCommand {
  /** Discriminator. */
  type: 'set_name';
  /** The requested display name; the server trims and validates before storing. */
  name: string;
}

/**
 * Request the public card catalog and per-format deck rules (issue #367). The server
 * replies with a one-shot {@link CatalogView}} and changes no lobby state, so a
 * connection can browse the supported card pool without joining or starting a game.
 */
export interface RequestCatalogCommand {
  /** Discriminator. */
  type: 'request_catalog';
}

/** Leave the current room, vacating the seat. */
export interface LeaveCommand {
  /** Discriminator. */
  type: 'leave';
}

/**
 * Everything a client can send in the lobby phase, a single tagged message
 * structurally parallel to {@link ChooseAction}}. The server validates every
 * command against authoritative state and answers with a fresh {@link LobbyView}};
 * an invalid command is rejected and the current `LobbyView` re-sent.
 */
export type LobbyCommand =
  | HelloCommand
  | CreateRoomCommand
  | JoinRoomCommand
  | SpectateRoomCommand
  | SubmitDeckCommand
  | AddAiCommand
  | RemoveAiCommand
  | ReadyCommand
  | SetNameCommand
  | RequestCatalogCommand
  | LeaveCommand;

/** Build a `hello` command, including a prior token only when present. */
export function helloCommand(token?: SessionToken): HelloCommand {
  const message: HelloCommand = { type: 'hello' };
  if (token !== undefined && token !== '') message.token = token;
  return message;
}

/** Build a `create_room` command for the given config. */
export function createRoomCommand(config: RoomConfig): CreateRoomCommand {
  return { type: 'create_room', config };
}

/** Build a `join_room` command for the given room id. */
export function joinRoomCommand(roomId: RoomId): JoinRoomCommand {
  return { type: 'join_room', room_id: roomId };
}

/** Build a `spectate_room` command for the given room id (issue #351). */
export function spectateRoomCommand(roomId: RoomId): SpectateRoomCommand {
  return { type: 'spectate_room', room_id: roomId };
}

/**
 * Build a `submit_deck` command, eliding `cards` when the decklist is empty and
 * `commander` when none is designated (issue #372). The designation is a bare
 * `CardIdentity`; the server validates it.
 */
export function submitDeckCommand(
  cards: CardIdentity[],
  commander?: CardIdentity,
): SubmitDeckCommand {
  const message: SubmitDeckCommand = { type: 'submit_deck' };
  if (cards.length > 0) message.cards = cards;
  if (commander) message.commander = commander;
  return message;
}

/**
 * Build an `add_ai` command seating an AI of `kind` in `seat` with the given deck (issue
 * #415), eliding `cards` when empty and `commander` when none is designated — the same
 * shape a `submit_deck` uses.
 */
export function addAiCommand(
  seat: number,
  kind: string,
  cards: CardIdentity[],
  commander?: CardIdentity,
): AddAiCommand {
  const message: AddAiCommand = { type: 'add_ai', seat, kind };
  if (cards.length > 0) message.cards = cards;
  if (commander) message.commander = commander;
  return message;
}

/** Build a `remove_ai` command emptying the AI in `seat` (issue #415). */
export function removeAiCommand(seat: number): RemoveAiCommand {
  return { type: 'remove_ai', seat };
}

/** Build a `ready` command declaring (`true`) or retracting (`false`) readiness. */
export function readyCommand(ready: boolean): ReadyCommand {
  return { type: 'ready', ready };
}

/** Build a `set_name` command requesting the given display name (issue #294). */
export function setNameCommand(name: string): SetNameCommand {
  return { type: 'set_name', name };
}

/** Build a `request_catalog` command (issue #367). */
export function requestCatalogCommand(): RequestCatalogCommand {
  return { type: 'request_catalog' };
}

/** Build a `leave` command. */
export function leaveCommand(): LeaveCommand {
  return { type: 'leave' };
}
