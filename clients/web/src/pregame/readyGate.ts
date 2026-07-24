/**
 * The ready gate, stated in words (issue #506;
 * `docs/design/front-door-and-lobby.md` §5.3, fixes P4).
 *
 * The shipped lobby drew the gate nowhere: deck state was a chip in the roster,
 * the deck picker was a separate section, and the CTA silently relabelled from
 * *Submit deck* to *Ready*. Nothing connected "you are not ready" to "your deck
 * is not submitted". This module derives that missing sentence — and which
 * single control is gold — from the **current `LobbyView` alone**.
 *
 * It is a pure presentation read: counts, names, and the gate sentence are
 * projections of the view the server sent. **No legality is computed here** —
 * `gold` only ever names a command the server already advertised in
 * `valid_commands`, and deck legality stays server-side behind the unchanged
 * `submit_deck` gate. Nothing is stored: the sentence is recomputed from every
 * frame, so it can never disagree with the roster beside it.
 */
import type { LobbyView, SeatView } from '../protocol';
import { seatDisplayName } from '../playerNames';
import { seatFilled } from './seatIdentity';

/** Which advertised command the room's single gold control offers, if any. */
export type ReadyGateGold = 'submit_deck' | 'ready' | null;

/** The gate as the ready bar renders it. */
export interface ReadyGateState {
  /** The gate in words — the reason the gold control is what it is. */
  sentence: string;
  /** The advertised command the one gold control sends (`null` ⇒ no gold). */
  gold: ReadyGateGold;
  /** Whether the quiet "Not ready" fallback is offered beside it. */
  unready: boolean;
  /** Whether the local seat has readied (drives the waiting treatment). */
  ready: boolean;
  /** Every seat filled, decked, and ready: the game is being constructed. */
  starting: boolean;
  /** Seats still to fill — presentation only. */
  openSeats: number;
}

/** Whether a command kind is currently offered to this connection. */
function can(view: LobbyView, command: string): boolean {
  return view.valid_commands.includes(command);
}

/** `1 player` / `N players`, so the sentence never reads "1 players". */
function players(count: number): string {
  return count === 1 ? '1 more player' : `${count} more players`;
}

/**
 * The occupants we are waiting on, as a readable list: one name, two joined by
 * "and", three or more elided to "and N others" so the bar never grows a
 * paragraph at eight seats.
 */
export function waitingForNames(names: readonly string[]): string {
  if (names.length === 0) return '';
  if (names.length === 1) return names[0]!;
  if (names.length === 2) return `${names[0]} and ${names[1]}`;
  const others = names.length - 2;
  return `${names[0]}, ${names[1]} and ${others === 1 ? '1 other' : `${others} others`}`;
}

/** The filled seats that have not readied yet, by display name. */
function notReadyNames(seats: readonly SeatView[], you: string | undefined): string[] {
  return seats
    .filter((seat) => seatFilled(seat) && seat.ready !== true && seat.occupied_by !== you)
    .map(seatDisplayName);
}

/**
 * Derive the gate from one `LobbyView`. Returns `null` when the view is not in
 * a room (the Lobby place has no gate). Every branch is reachable from a real
 * frame; the five §8-criterion-9 states are: no deck submitted; deck submitted
 * and not ready; ready and waiting; waiting for players; starting.
 */
export function readyGate(view: LobbyView): ReadyGateState | null {
  const room = view.room;
  if (room === undefined) return null;

  const seats = room.seats;
  const total = seats.length;
  const filled = seats.filter(seatFilled).length;
  const openSeats = Math.max(0, total - filled);
  const mySeat = seats.find((seat) => seat.occupied_by === view.you);
  const decked = mySeat?.decked === true;
  const ready = mySeat?.ready === true;
  const canSubmit = can(view, 'submit_deck');
  const canReady = can(view, 'ready');
  const unready = can(view, 'unready');

  // Starting: every seat filled, decked, and ready. The server constructs the
  // game on this same frame, so the bar can say so without inventing a
  // countdown — it is reading the view, not predicting it.
  const starting =
    openSeats === 0 && seats.every((seat) => seat.decked === true && seat.ready === true);
  if (starting) {
    return { sentence: 'Starting the game…', gold: null, unready, ready, starting, openSeats };
  }

  // You are ready: the bar names who the room is still waiting on.
  if (ready) {
    const names = notReadyNames(seats, view.you);
    const sentence =
      openSeats > 0
        ? `You're ready — waiting for ${players(openSeats)} to join`
        : names.length > 0
          ? `You're ready — waiting for ${waitingForNames(names)}`
          : "You're ready — waiting for the other players";
    return { sentence, gold: null, unready, ready, starting, openSeats };
  }

  // Your deck is not in yet: that is the reason the gold control is Submit deck.
  if (!decked && canSubmit) {
    return {
      sentence: 'Choose and submit a deck',
      gold: 'submit_deck',
      unready,
      ready,
      starting,
      openSeats,
    };
  }

  // Decked (or nothing to submit) and not ready: the room is either still
  // filling or waiting on you to ready up.
  const gold: ReadyGateGold = canReady ? 'ready' : null;
  const sentence =
    openSeats > 0
      ? `Waiting for ${players(openSeats)} to join`
      : canReady
        ? "Everyone's here — ready up to start"
        : 'Waiting for the other players';
  return { sentence, gold, unready, ready, starting, openSeats };
}
