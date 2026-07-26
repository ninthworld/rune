/**
 * The shared wire-reading primitives every normalizer in this client builds on.
 *
 * Split out of {@link ./wire} so the presentation-moment reader (`wireMoments.ts`,
 * issue #594) can use the same guards without a cycle back through it, and so `wire.ts`
 * stays inside the file-size budget AGENTS.md sets. `wire.ts` re-exports
 * {@link ProtocolError} unchanged, so every existing `import { ProtocolError } from
 * './wire'` keeps naming this one class.
 *
 * Wire hygiene only: nothing here computes legality, cost, or effect, and unknown fields
 * are tolerated for forward compatibility.
 */
import { type GameResult, type PlayerId } from './protocol';

/** Raised when a server payload is not a decodable {@link GameView}. */
export class ProtocolError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'ProtocolError';
  }
}

/** Whether a wire value is a plain JSON object (not an array, not `null`). */
export function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

/** Coerce a wire value into a string, treating an omitted field as `''`. */
export function asString(value: unknown): string {
  return typeof value === 'string' ? value : '';
}

/**
 * Whether a wire value is a usable number: present, numeric, and finite. `NaN` and the
 * infinities are as unreadable as a missing field — a caller that defaulted one would be
 * carrying a quantity the server never stated.
 */
export function isFiniteNumber(value: unknown): value is number {
  return typeof value === 'number' && Number.isFinite(value);
}

/**
 * Coerce a wire value into an array, treating an omitted field (`undefined`) as the
 * documented empty default. A present-but-non-array value is a protocol violation and
 * throws.
 */
export function asArray<T>(value: unknown, field: string): T[] {
  if (value === undefined) return [];
  if (!Array.isArray(value)) {
    throw new ProtocolError(`GameView.${field} must be an array`);
  }
  return value as T[];
}

/**
 * Normalize the optional terminal {@link GameResult} carried by a view (and by a
 * `game_over` presentation moment, issue #594). Returns `undefined` while the game is
 * live — the server omits `result`, or sends a malformed object with no string `reason` —
 * so its mere presence signals game over. `losers` defaults to the empty array; `winner`
 * stays absent for a draw. The `reason` is carried through verbatim: the client renders
 * it and derives no terminality of its own, and an unrecognized future value is tolerated
 * (forward compatibility) and handled generically by the game-over overlay.
 */
export function normalizeGameResult(payload: unknown): GameResult | undefined {
  if (!isRecord(payload) || typeof payload.reason !== 'string') return undefined;
  const result: GameResult = {
    losers: asArray<PlayerId>(payload.losers, 'result.losers'),
    reason: payload.reason as GameResult['reason'],
  };
  if (typeof payload.winner === 'string') result.winner = payload.winner;
  return result;
}
