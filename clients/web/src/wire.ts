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
import { type GameView, PHASES, type Phase } from './protocol';

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
 * Normalize an already-parsed payload into a complete {@link GameView}. Missing
 * collections become empty arrays; optional scalars (`priority_player`,
 * `action_deadline`) are carried through untouched. Throws {@link ProtocolError}
 * if the payload is not an object or lacks a valid `phase`.
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
