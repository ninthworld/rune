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

/** The display name for a seat, falling back to the opaque id the server assigned. */
export function playerLabel(view: GameView, id: string): string {
  const name = view.player_names?.[id]
  return name ? `${name} (${id})` : id
}

/** Whether the receiver currently holds priority and may act. */
export const hasPriority = (view: GameView): boolean =>
  view.priority_player !== undefined && view.priority_player === view.you

/** Permanents a given seat controls, in the order the server listed them. */
export const controlledBy = (view: GameView, player: string) =>
  list(view.battlefield).filter((permanent) => permanent.controller === player)

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
