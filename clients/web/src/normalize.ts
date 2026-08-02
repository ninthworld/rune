/**
 * Turning wire absence into values a renderer can use.
 *
 * `protocol.ts` declares no defaults on purpose: the mirror reports exactly what the server
 * said, and the parity test depends on that. Every default the protocol documents is applied
 * here instead, in one place, so no component invents its own reading of a missing field.
 *
 * Two of these are not cosmetic:
 *
 * - **`connected` absent means connected.** The flag rides the wire only when `false`, so the
 *   common case is absence. Reading it as `false` would paint every healthy seat as dropped.
 * - **An absent collection and an empty one mean the same thing.** The server elides an empty
 *   list, but a list may also arrive explicitly empty — a finished game carries
 *   `valid_actions: []`. A renderer that distinguished them would show a concluded game as
 *   still offering moves.
 */
import type { GameView, OpponentView, SelfView } from './protocol'

export const list = <T>(value: readonly T[] | undefined): readonly T[] => value ?? []

/** A seat is connected unless the server explicitly said otherwise. */
export const isConnected = (seat: Pick<SelfView | OpponentView, 'connected'>): boolean =>
  seat.connected !== false

/**
 * Every seat this view seats, in the order it seated them.
 *
 * `seat_order` is the authority when it is present. When it is not — an older frame, or a view
 * that simply did not carry it — the seats are still known from `you` and `opponents[]`, and
 * reconstructing them is better than having no table at all. What is never done is inventing an
 * order the server did not state for seats it did not list.
 */
export function seatOrder(view: GameView): readonly string[] {
  const stated = list(view.seat_order)
  if (stated.length > 0) return stated
  const known = [
    ...(view.you === undefined ? [] : [view.you]),
    ...list(view.opponents).map((opponent) => opponent.player_id),
  ]
  return known.filter((id, index) => known.indexOf(id) === index)
}

/**
 * What a seat is called on every surface that names one.
 *
 * The server's display name, verbatim, and nothing else beside it. A seat id is a wire key: the
 * player has never seen the protocol, and `Bo (p2)` prints the half of that sentence they have no
 * use for (`docs/client-design.md` §2.1, rule 3 — a seat id belongs in a log).
 *
 * The two cases where a name alone will not do are answered from the view and from nothing else:
 *
 * - **Nobody named this seat.** `player_names` is elided when empty, so a whole table of nameless
 *   seats is ordinary. The view still states which seat is the reader's, and *you* and *your
 *   opponent* is how a player already thinks about a two-seat table — so that is what those seats
 *   read as, on the panel, the field, the life total and the trail alike. With no seat stated as
 *   the reader's — a spectator — there is no opponent to speak of and the seats read by position
 *   instead, because calling somebody else's game "opponent" is a fact this view did not state.
 * - **Two seats share a name.** The server allows it deliberately (`validate_name`: "two Alices
 *   are disambiguated by their seat"), so it is reachable and the label has to survive it. The
 *   qualifier is the seat's position at the table, which is a thing a player can see; it is
 *   never the id, which is the whole point of this function.
 *
 * Nothing here concludes anything about a seat that the view did not state. Position is
 * `seat_order`'s, being-yours is `you`'s, and a name is the server's.
 */
export function playerLabel(view: GameView, id: string): string {
  const name = view.player_names?.[id]
  if (name === undefined || name === '') return unnamedSeat(view, id)
  const shared = Object.entries(view.player_names ?? {}).some(
    ([seat, other]) => seat !== id && other === name,
  )
  if (!shared) return name
  const seat = seatNumber(view, id)
  return seat === undefined ? name : `${name} (seat ${seat})`
}

/** Whether the server stated a display name for this seat — the label is its own otherwise. */
export const isNamed = (view: GameView, id: string): boolean => {
  const name = view.player_names?.[id]
  return name !== undefined && name !== ''
}

/** A seat's 1-based position in the order the server seated the table, if it stated one. */
function seatNumber(view: GameView, id: string): number | undefined {
  const at = seatOrder(view).indexOf(id)
  return at < 0 ? undefined : at + 1
}

/** What a seat nobody named reads as: who they are to the reader, and where they sit. */
function unnamedSeat(view: GameView, id: string): string {
  const number = seatNumber(view, id)
  // No seat is the reader's, so no seat is anybody's opponent: this is a spectator, or a frame
  // that never said. Position is all that is left, and it is all that is claimed.
  if (view.you === undefined) return number === undefined ? 'Seat' : `Seat ${number}`
  if (id === view.you) return 'You'
  // One other seat is *the* opponent; several have to be told apart, and the number that does it
  // is the seat's own position rather than a count invented here.
  const others = seatOrder(view).filter((seat) => seat !== view.you)
  if (others.length <= 1 || number === undefined) return 'Opponent'
  return `Opponent ${number}`
}

/**
 * The objects a given seat controls, in the order the server listed them.
 *
 * Takes the list rather than the view because more than one thing on screen is grouped by
 * controller — permanents, emblems, and whatever a caller has already paired with its rendered
 * face — and all of them should group the same way.
 */
export const controlledBy = <T extends { controller: string }>(
  items: readonly T[],
  player: string,
): readonly T[] => items.filter((item) => item.controller === player)

/**
 * A card's power/toughness as displayed, or `undefined` for a non-creature.
 *
 * Both values are strings on the wire because they are **server-computed** — the effective
 * value after counters, anthems, and pumps, not the printed one. The client never recomputes
 * them and never renders the printed value in their place.
 */
export function powerToughness(card: { power?: string; toughness?: string }): string | undefined {
  if (card.power === undefined || card.toughness === undefined) return undefined
  return `${card.power}/${card.toughness}`
}

/** A short, readable summary of one seat for the grey-box header. */
export function seatSummary(seat: SelfView | OpponentView): string {
  const parts = [`${seat.life} life`, `${seat.library_size} library`]
  if ('hand_size' in seat) parts.push(`${seat.hand_size} hand`)
  if ('graveyard_size' in seat) parts.push(`${seat.graveyard_size} graveyard`)
  if (seat.eliminated) parts.push('eliminated')
  if (!isConnected(seat)) parts.push('disconnected')
  if (seat.ai) parts.push('AI')
  return parts.join(' · ')
}
