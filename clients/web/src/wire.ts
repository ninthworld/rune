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
  type AiOption,
  type CardView,
  type CatalogCard,
  type CatalogFormat,
  type CatalogView,
  type CommanderDamage,
  type CommanderTax,
  type Counter,
  type GameResult,
  type GameView,
  type LobbyRejection,
  type LobbyView,
  type Permanent,
  PHASES,
  type Phase,
  type PlayerId,
  type RoomConfig,
  type RoomState,
  type RoomSummary,
  type RoomView,
  type SeatView,
  type SelfView,
  type SpectatorView,
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
 * Coerce a wire value into a string→string map, treating an omitted field as the
 * empty map and dropping any non-string entry. Used for `GameView.player_names`
 * (issue #294): the server elides it when empty, so a missing or malformed value
 * degrades to `{}` rather than throwing — the client then falls back per player.
 */
function normalizeStringMap(value: unknown): Record<string, string> {
  if (!isRecord(value)) return {};
  const out: Record<string, string> = {};
  for (const [key, entry] of Object.entries(value)) {
    if (typeof entry === 'string') out[key] = entry;
  }
  return out;
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
 * Normalize one wire {@link Permanent}. The server elides absent optionals, so a
 * missing `tapped`/`attached_to`/`counters` stays absent rather than being invented,
 * keeping the normalized shape identical to the terse wire shape. The computed
 * `card` face rides through untouched — the client renders exactly what the server
 * sent and derives no characteristics. This is the single per-permanent
 * normalization point every combat/attachment field flows through.
 */
function normalizePermanent(payload: unknown): Permanent {
  const record = isRecord(payload) ? payload : {};
  const perm: Permanent = {
    id: asString(record.id),
    controller: asString(record.controller),
    owner: asString(record.owner),
    card: record.card as CardView,
  };
  if (record.tapped === true) perm.tapped = true;
  // Combat declaration state (issue #332, CR 508/509): whether this permanent is
  // attacking, and which attacker it is blocking. Present only mid-combat; a view
  // that omits them (not in combat, or an older server) defaults to not-in-combat.
  if (record.attacking === true) perm.attacking = true;
  // The defending player an attacker attacks (issue #341/#345): present only mid-combat
  // in a multiplayer game; a two-player or older view omits it and the client falls
  // back to the sole opponent.
  if (typeof record.attacking_player === 'string') perm.attacking_player = record.attacking_player;
  if (typeof record.blocking === 'string') perm.blocking = record.blocking;
  // Marked combat damage (issue #332, CR 120.3): a non-negative number, present only
  // while damage is marked; an omitted or non-positive value defaults to undamaged.
  if (typeof record.damage === 'number' && record.damage > 0) perm.damage = record.damage;
  // Aura attachment (issue #333): the host's entity id, present only when attached;
  // a view that omits it (older server) degrades to an unattached permanent.
  if (typeof record.attached_to === 'string') perm.attached_to = record.attached_to;
  if (Array.isArray(record.counters)) perm.counters = record.counters as Counter[];
  return perm;
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
 * Normalize the public commander-damage tally (CR 903.10a, issue #371). The server
 * elides it when empty, so a missing or malformed value degrades to `[]`; each
 * entry keeps only the well-formed `(commander, damaged, amount)` triple, dropping
 * anything else so an unexpected shape never breaks rendering. Wire hygiene, not
 * game logic — the client renders these numbers and derives no terminality itself.
 */
function normalizeCommanderDamage(value: unknown): CommanderDamage[] {
  if (!Array.isArray(value)) return [];
  const out: CommanderDamage[] = [];
  for (const raw of value) {
    if (!isRecord(raw)) continue;
    if (
      typeof raw.commander === 'string' &&
      typeof raw.damaged === 'string' &&
      typeof raw.amount === 'number'
    ) {
      out.push({ commander: raw.commander, damaged: raw.damaged, amount: raw.amount });
    }
  }
  return out;
}

/**
 * Normalize the public commander-tax list (CR 903.8, issue #372). The server elides
 * it when empty, so a missing or malformed value degrades to `[]`; each entry keeps
 * a well-formed `commander` id and its numeric `casts`/`tax` (defaulting each to `0`
 * when the server elides a zero), dropping anything else so an unexpected shape never
 * breaks rendering. Wire hygiene, not game logic — the client renders these numbers.
 */
function normalizeCommanderTax(value: unknown): CommanderTax[] {
  if (!Array.isArray(value)) return [];
  const out: CommanderTax[] = [];
  for (const raw of value) {
    if (!isRecord(raw)) continue;
    if (typeof raw.commander === 'string') {
      out.push({
        commander: raw.commander,
        casts: typeof raw.casts === 'number' ? raw.casts : 0,
        tax: typeof raw.tax === 'number' ? raw.tax : 0,
      });
    }
  }
  return out;
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
    battlefield: asArray<unknown>(payload.battlefield, 'battlefield').map(normalizePermanent),
    stack: asArray(payload.stack, 'stack'),
    graveyards: asArray(payload.graveyards, 'graveyards'),
    exile: asArray(payload.exile, 'exile'),
    // Command zone (CR 903.6, issue #372): the same public pile treatment as
    // graveyards/exile; elided (defaults to `[]`) for a non-commander game.
    command: asArray(payload.command, 'command'),
    phase: payload.phase,
    // Turn structure (issue #267): the server owns turn counting and whose turn it
    // is; an older server may omit them, so default to 0/'' (unknown).
    turn: typeof payload.turn === 'number' ? payload.turn : 0,
    active_player: typeof payload.active_player === 'string' ? payload.active_player : '',
    // Explicit seat order (issue #345): every seat id in order; an older server omits
    // it, defaulting to `[]` (the client then falls back to its prior inference).
    seat_order: asArray(payload.seat_order, 'seat_order'),
    mana_pool: asArray(payload.mana_pool, 'mana_pool'),
    priority_player:
      typeof payload.priority_player === 'string' ? payload.priority_player : undefined,
    valid_actions: asArray(payload.valid_actions, 'valid_actions'),
    action_deadline:
      typeof payload.action_deadline === 'number' ? payload.action_deadline : undefined,
    result: normalizeGameResult(payload.result),
    log: asArray(payload.log, 'log'),
    // Priority-stop preferences (issue #264): a list of phase names the server elides
    // when empty; keep only recognized phases so an unknown future value never breaks
    // rendering, defaulting to `[]` (stop nowhere).
    stops: normalizePhaseList(payload.stops),
    // The auto-pass indicator (issue #264): display-only, defaults to `false` when the
    // seat was not auto-passed (or an older server omits it).
    auto_passed: payload.auto_passed === true,
    // Rejected-action feedback (issue #265): display-only, defaults to `false` on every
    // normal broadcast/resync (or when an older server omits it). Only the one re-send
    // answering a rejected action sets it, driving a transient toast.
    action_rejected: payload.action_rejected === true,
    // Public display names (issue #294): a string→string map the server elides when
    // empty; default to `{}` so every surface can look a name up and fall back when
    // absent (older servers never send it).
    player_names: normalizeStringMap(payload.player_names),
    // Commander combat-damage tally (issue #371): public, elided when empty; default
    // to `[]` so a non-commander game and an older server are unchanged.
    commander_damage: normalizeCommanderDamage(payload.commander_damage),
    // Commander tax (issue #372): public, elided when empty; default to `[]`.
    commander_tax: normalizeCommanderTax(payload.commander_tax),
  };
}

/**
 * Normalize a raw wire {@link SpectatorView} (ADR 0022, issue #351): the public
 * intersection only. It has no receiver/decision fields to default — the type
 * literally cannot hold a hand, a mana pool, or a `valid_actions` list — so this
 * mirrors {@link normalizeGameView} minus every private field. A missing collection
 * degrades to `[]`; the required `phase` throws if absent, like a `GameView`.
 */
export function normalizeSpectatorView(payload: unknown): SpectatorView {
  if (!isRecord(payload)) {
    throw new ProtocolError('SpectatorView payload must be a JSON object');
  }
  if (!isPhase(payload.phase)) {
    throw new ProtocolError(`SpectatorView.phase is missing or invalid: ${String(payload.phase)}`);
  }
  return {
    players: asArray(payload.players, 'players'),
    battlefield: asArray<unknown>(payload.battlefield, 'battlefield').map(normalizePermanent),
    stack: asArray(payload.stack, 'stack'),
    graveyards: asArray(payload.graveyards, 'graveyards'),
    exile: asArray(payload.exile, 'exile'),
    // Command zone (issue #372): the same public pile seated views carry.
    command: asArray(payload.command, 'command'),
    phase: payload.phase,
    turn: typeof payload.turn === 'number' ? payload.turn : 0,
    active_player: typeof payload.active_player === 'string' ? payload.active_player : '',
    seat_order: asArray(payload.seat_order, 'seat_order'),
    priority_player:
      typeof payload.priority_player === 'string' ? payload.priority_player : undefined,
    result: normalizeGameResult(payload.result),
    log: asArray(payload.log, 'log'),
    player_names: normalizeStringMap(payload.player_names),
    // Commander combat-damage tally (issue #371): the same public list seated views
    // carry, elided when empty.
    commander_damage: normalizeCommanderDamage(payload.commander_damage),
    // Commander tax (issue #372): the same public list seated views carry.
    commander_tax: normalizeCommanderTax(payload.commander_tax),
  };
}

/**
 * Coerce a wire value into a list of known {@link Phase} values, dropping any
 * non-phase entry. Used for `GameView.stops` (issue #264): the server elides it when
 * empty, so a missing or malformed value degrades to `[]` and an unrecognized future
 * phase is simply ignored rather than throwing.
 */
function normalizePhaseList(value: unknown): Phase[] {
  if (!Array.isArray(value)) return [];
  return value.filter(isPhase);
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
  // Display name (issue #294): present only when the occupant has named themselves.
  if (typeof record.name === 'string') seat.name = record.name;
  // AI opponent kind (issue #415): present only when the seat is filled by an AI.
  if (typeof record.ai === 'string') seat.ai = record.ai;
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

/** The room lifecycle states the directory knows; anything else defaults to
 * `'gathering'` so an unknown future value never breaks rendering. */
function normalizeRoomState(value: unknown): RoomState {
  return value === 'in_progress' ? 'in_progress' : 'gathering';
}

/**
 * Normalize one wire {@link RoomSummary} (a directory entry). Missing fields default
 * to their empty/zero form; `state` falls back to `'gathering'` for an unknown value.
 */
function normalizeRoomSummary(payload: unknown): RoomSummary {
  const record = isRecord(payload) ? payload : {};
  return {
    room_id: asString(record.room_id),
    config: normalizeRoomConfig(record.config),
    filled: typeof record.filled === 'number' ? record.filled : 0,
    // Spectator count (issue #351): the server elides it when zero.
    spectators: typeof record.spectators === 'number' ? record.spectators : 0,
    state: normalizeRoomState(record.state),
  };
}

/**
 * Normalize an already-parsed payload into a complete {@link LobbyView}. Missing
 * `session`/`you` default to `''` (like `GameView.you`), `room` stays absent when
 * omitted, and `directory`/`valid_commands` become the empty array. This is wire
 * hygiene, not game logic — unknown fields are tolerated for forward compatibility.
 */
export function normalizeLobbyView(payload: unknown): LobbyView {
  if (!isRecord(payload)) {
    throw new ProtocolError('LobbyView payload must be a JSON object');
  }
  const view: LobbyView = {
    session: asString(payload.session),
    you: asString(payload.you),
    directory: asArray(payload.directory, 'directory').map(normalizeRoomSummary),
    valid_commands: asArray<string>(payload.valid_commands, 'valid_commands'),
  };
  // The connection's own display name (issue #294): present only once set.
  if (typeof payload.name === 'string') view.name = payload.name;
  if (isRecord(payload.room)) view.room = normalizeRoomView(payload.room);
  return view;
}

/** Normalize one wire {@link CatalogCard}, defaulting elided optionals (issue #367). */
function normalizeCatalogCard(payload: unknown): CatalogCard {
  const record = isRecord(payload) ? payload : {};
  const card: CatalogCard = {
    functional_id: asString(record.functional_id),
    name: asString(record.name),
    type_line: asString(record.type_line),
    rules_text: typeof record.rules_text === 'string' ? record.rules_text : '',
  };
  if (typeof record.mana_cost === 'string') card.mana_cost = record.mana_cost;
  if (typeof record.power === 'string') card.power = record.power;
  if (typeof record.toughness === 'string') card.toughness = record.toughness;
  if (Array.isArray(record.keywords)) {
    card.keywords = record.keywords.filter((k): k is string => typeof k === 'string');
  }
  return card;
}

/** Normalize one wire {@link CatalogFormat}; an absent upper bound stays absent
 * (`None` = no limit), never invented, so permissiveness reads honestly (issue #367). */
function normalizeCatalogFormat(payload: unknown): CatalogFormat {
  const record = isRecord(payload) ? payload : {};
  const format: CatalogFormat = {
    game_setup: asString(record.game_setup),
    min_deck_size: typeof record.min_deck_size === 'number' ? record.min_deck_size : 0,
    basic_land_exempt: record.basic_land_exempt === true,
    requires_commander: record.requires_commander === true,
    enforce_color_identity: record.enforce_color_identity === true,
    min_seats: typeof record.min_seats === 'number' ? record.min_seats : 0,
    max_seats: typeof record.max_seats === 'number' ? record.max_seats : 0,
  };
  if (typeof record.max_deck_size === 'number') format.max_deck_size = record.max_deck_size;
  if (typeof record.max_copies === 'number') format.max_copies = record.max_copies;
  return format;
}

/**
 * Normalize an already-parsed payload into a complete {@link CatalogView} (issue #367).
 * Missing `cards`/`formats` become empty arrays and elided per-entry optionals default,
 * exactly like every other view. Throws {@link ProtocolError} for a non-object payload.
 */
export function normalizeCatalogView(payload: unknown): CatalogView {
  if (!isRecord(payload)) {
    throw new ProtocolError('CatalogView payload must be a JSON object');
  }
  return {
    catalog_version: typeof payload.catalog_version === 'number' ? payload.catalog_version : 0,
    cards: asArray<unknown>(payload.cards, 'cards').map(normalizeCatalogCard),
    formats: asArray<unknown>(payload.formats, 'formats').map(normalizeCatalogFormat),
    // AI opponent kinds (issue #415): a missing field is treated as an empty list, so an
    // older server that ships none leaves the seating UI simply offering nothing.
    ai_opponents: asArray<unknown>(payload.ai_opponents ?? [], 'ai_opponents').map(
      normalizeAiOption,
    ),
  };
}

/** Normalize one wire {@link AiOption} (issue #415); an absent description stays absent. */
function normalizeAiOption(payload: unknown): AiOption {
  const record = isRecord(payload) ? payload : {};
  const option: AiOption = {
    id: asString(record.id),
    name: asString(record.name),
  };
  if (typeof record.description === 'string') option.description = record.description;
  return option;
}

/**
 * Normalize a wire {@link LobbyRejection} (issue #395): `code`/`reason` default to
 * `''` and `card` stays absent when the rejection names no specific card. Wire
 * hygiene only — the reason is displayed verbatim and no legality is derived.
 */
function normalizeLobbyRejection(payload: unknown): LobbyRejection {
  const record = isRecord(payload) ? payload : {};
  const rejection: LobbyRejection = {
    code: asString(record.code),
    reason: asString(record.reason),
  };
  if (typeof record.card === 'string') rejection.card = record.card;
  return rejection;
}

/**
 * One decoded server→client frame: an in-game {@link GameView}, a
 * {@link SpectatorView}, a pre-game {@link LobbyView}, a {@link CatalogView}, or a
 * {@link LobbyErrorFrame}. They are distinguished structurally — a
 * `GameView`/`SpectatorView` carries a valid {@link Phase}, a `CatalogView` carries a
 * `catalog_version` (and no phase), a lobby-error frame carries a `lobby_error`
 * object, and a `LobbyView` carries none of these — so a single connection can carry
 * all of them.
 */
export type ServerFrame =
  | { readonly kind: 'game'; readonly view: GameView }
  | { readonly kind: 'spectator'; readonly view: SpectatorView }
  | { readonly kind: 'catalog'; readonly catalog: CatalogView }
  | { readonly kind: 'lobby_error'; readonly rejection: LobbyRejection }
  | { readonly kind: 'lobby'; readonly lobby: LobbyView };

/**
 * Parse a raw server→client text frame, routing it to a {@link GameView}, a
 * {@link SpectatorView}, or a {@link LobbyView}. A frame with a valid `phase` is an
 * in-game view; a {@link SpectatorView} is distinguished by its spectator-only
 * `players` array together with the absence of a receiver `you` (ADR 0022, issue #351)
 * — a seated {@link GameView} carries `opponents`/`you`, never `players`, so a
 * `you`-less older-server game frame still routes to a `GameView`. A frame with no
 * `phase` is a `LobbyView`. Throws {@link ProtocolError} on malformed JSON or a
 * non-object payload.
 */
export function parseServerFrame(raw: string): ServerFrame {
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch (cause) {
    throw new ProtocolError(`server frame is not valid JSON: ${String(cause)}`);
  }
  if (isRecord(parsed) && isPhase(parsed.phase)) {
    // A spectator frame carries the spectator-only `players` array and no receiver
    // `you`; anything else with a phase is a seated game view.
    if ('players' in parsed && !('you' in parsed)) {
      return { kind: 'spectator', view: normalizeSpectatorView(parsed) };
    }
    return { kind: 'game', view: normalizeGameView(parsed) };
  }
  // A catalog frame (issue #367) carries a `catalog_version` and no phase; a `LobbyView`
  // carries neither, so the version is an unambiguous discriminator between the two.
  if (isRecord(parsed) && typeof parsed.catalog_version === 'number') {
    return { kind: 'catalog', catalog: normalizeCatalogView(parsed) };
  }
  // A lobby-error frame (issue #395) carries a `lobby_error` object and no phase; no
  // `LobbyView` carries that key, so it is an unambiguous discriminator.
  if (isRecord(parsed) && isRecord(parsed.lobby_error)) {
    return { kind: 'lobby_error', rejection: normalizeLobbyRejection(parsed.lobby_error) };
  }
  return { kind: 'lobby', lobby: normalizeLobbyView(parsed) };
}
