/**
 * One seat's **authoritative presentation state** (issue #553), read straight off a
 * `GameView` or a `SpectatorView`.
 *
 * `seat-identity.md` §11 lists five slots that were deliberately dormant because no
 * field carried them: the disconnected link glyph, the *local* eliminated treatment,
 * a colour identity that survives the command zone, the commander's name on the
 * nameplate, and the AI-seat marker. This module is the data source for all five,
 * and it exists mainly to make the reading *correct*:
 *
 * - `connected` is the one wire flag whose **absent value is `true`** (it rides the
 *   wire only as `false`), so a bare `if (!seat.connected)` would report every
 *   connected seat as disconnected. That inversion is handled here, once.
 * - the receiver's own state lives on `me`, every other seat's on `opponents[]` (or,
 *   for a spectator, on `players[]`); a caller should not have to branch on that.
 *
 * **No inference happens here.** Every value is a field the server sent; a slot with
 * no field stays at its documented default, and nothing is derived from a name, a
 * mana cost, a zone, or the log. In particular the commander's colour identity is
 * the server's CR 903.4 computation — never `deriveColorIdentity()`, which frames a
 * *card* from its printed cost and knows nothing about a commander.
 */
import type { Color, CommanderIdentity, GameView, OpponentView, PlayerId } from '../protocol';
import type { SpectatorView } from '../protocol';

/** Everything a seat cluster renders about one seat's state, all server-stated. */
export interface SeatPresentation {
  /** Whether the seat has a live connection. A disconnected seat is held open. */
  connected: boolean;
  /** Whether the seat lost and left the game (CR 800.4a) while play continues. */
  eliminated: boolean;
  /** Whether the seat is played by a server-side AI. */
  ai: boolean;
  /** The seat's commander's name (CR 903.3), or `null` when it designated none. */
  commanderName: string | null;
  /**
   * The seat's commander colour identity (CR 903.4) in WUBRG order. Empty both for a
   * seat with no commander and for a **colourless** commander — those are the same
   * thing to render (no gems), and the distinction is `commanderName !== null`.
   */
  colorIdentity: readonly Color[];
}

/** The state of a seat nothing is known about: present, alive, human, commanderless. */
const UNKNOWN_SEAT: SeatPresentation = {
  connected: true,
  eliminated: false,
  ai: false,
  commanderName: null,
  colorIdentity: [],
};

/**
 * Whether this match is played under the Commander rules (CR 903), from the server's
 * `format` signal alone.
 *
 * Deliberately **not** derived from `command[]`, `commander_tax[]`, or
 * `commander_damage[]`: all three are legitimately empty in ordinary Commander states
 * (every commander on the battlefield, none cast yet, no combat damage through), so
 * every zone-shaped guess is wrong exactly when it matters. An absent `format` means
 * "unknown, not Commander" — the pre-#553 reading.
 */
export function isCommanderMatch(view: Pick<GameView, 'format'>): boolean {
  return view.format?.commander === true;
}

/** The commander identity entry for `seat`, or `undefined` when it designated none. */
function identityFor(
  identities: readonly CommanderIdentity[] | undefined,
  seat: PlayerId,
): CommanderIdentity | undefined {
  return identities?.find((entry) => entry.commander === seat);
}

/** The public half of a seat record, shared by `OpponentView` and `SelfView`. */
function presentationOf(
  seat: PlayerId,
  record: { eliminated?: boolean; connected?: boolean; ai?: boolean },
  identities: readonly CommanderIdentity[] | undefined,
): SeatPresentation {
  const identity = identityFor(identities, seat);
  return {
    // Absent means connected: only an explicit `false` marks a dropped seat.
    connected: record.connected !== false,
    eliminated: record.eliminated === true,
    ai: record.ai === true,
    commanderName: identity?.name ?? null,
    colorIdentity: identity?.color_identity ?? [],
  };
}

/**
 * One seat's presentation state from a seated {@link GameView}. The receiver reads
 * its own state off `me`; every other seat off its `opponents[]` entry. A seat the
 * view does not describe falls back to the documented defaults rather than to a
 * guess or an "unknown" placeholder.
 */
export function seatPresentation(view: GameView, seat: PlayerId): SeatPresentation {
  if (seat !== '' && seat === view.you) {
    return presentationOf(seat, view.me, view.commander_identity);
  }
  const opponent: OpponentView | undefined = view.opponents.find((o) => o.player_id === seat);
  if (opponent === undefined) return UNKNOWN_SEAT;
  return presentationOf(seat, opponent, view.commander_identity);
}

/**
 * The spectator counterpart: a `SpectatorView` has no privileged self, so every seat
 * is read from the same public `players[]` list.
 */
export function spectatorSeatPresentation(view: SpectatorView, seat: PlayerId): SeatPresentation {
  const player = view.players.find((p) => p.player_id === seat);
  if (player === undefined) return UNKNOWN_SEAT;
  return presentationOf(seat, player, view.commander_identity);
}
