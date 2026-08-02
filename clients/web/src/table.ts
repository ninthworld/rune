/**
 * The table, as one model: who is sitting at it and what is in front of each of them.
 *
 * A seat's facts arrive scattered across the view. Life and library come from `me` for you and
 * from `opponents[]` for everyone else — two shapes that deliberately differ, because one of
 * them is redacted. The piles arrive as three separate arrays keyed by player. Commander
 * damage, tax, and identity arrive as three more. A component that read all six would be
 * rebuilding this join every time it painted, and each surface would get the absences slightly
 * differently wrong.
 *
 * So it is joined once, here, and the surfaces render `Seat[]`.
 *
 * The redaction is preserved rather than smoothed over: an opponent has a `handSize` and no
 * hand, you have a hand and no `handSize`, and neither one is invented for the other. Where the
 * server projected nothing — a seat it has not sent totals for — the field stays absent and the
 * panel shows nothing rather than a zero that would read as a real number.
 */
import type { GameView } from './protocol'
import { cardFace, type CardFace } from './card-face'
import { isConnected, isNamed, list, playerLabel, seatOrder } from './normalize'

/**
 * One pip of unspent mana.
 *
 * `mana_pool` is a list of pip strings, and a pip suffixed `*` is **restricted** mana (CR 106.6,
 * `docs/protocol.md`): it may be spent only on whatever produced it. That suffix is a wire
 * encoding, not part of the symbol, so it is read here once rather than reaching a component as
 * a bare asterisk nobody can explain.
 *
 * What the wire says is *that* a pip is restricted, never what to — so a client can say a pip is
 * restricted and must not guess at the condition.
 */
export interface ManaPip {
  /** The pip as printed, e.g. `{G}`. */
  symbol: string
  restricted: boolean
}

/** Split one wire pip into its symbol and its restriction. */
export const manaPip = (pip: string): ManaPip =>
  pip.endsWith('*')
    ? { symbol: pip.slice(0, -1), restricted: true }
    : { symbol: pip, restricted: false }

/** One public pile in front of a seat. The library is a count, so it is not one of these. */
export interface SeatPile {
  zone: 'graveyard' | 'exile' | 'command'
  label: string
  faces: readonly CardFace[]
}

/** Everything the table shows about one seat. */
export interface Seat {
  id: string
  name: string
  /**
   * Whether `name` is the server's word for this seat or the client's.
   *
   * A seat nobody named is called `You` or `Opponent` (`normalize.playerLabel`), which already
   * says whose it is — so a surface that would otherwise add that fact a second time can tell
   * the two apart without comparing the string against a phrase.
   */
  named: boolean
  isYou: boolean
  /** Absent when the server projected no totals for this seat at all. */
  life?: number
  librarySize?: number
  /** An opponent's hand is a count. Yours is `my_hand`, and is not counted here. */
  handSize?: number
  graveyardSize?: number
  statuses: readonly string[]
  eliminated: boolean
  connected: boolean
  ai: boolean
  piles: readonly SeatPile[]
  /** Only ever your own: the server sends no one else's floating mana. */
  manaPool: readonly ManaPip[]
  /** Commander (CR 903). All three are absent outside a Commander game. */
  commanderName?: string
  commanderTax?: number
  commanderCasts?: number
  /** Damage this seat has taken from each commander, by that commander's controller. */
  commanderDamage: readonly { from: string; amount: number }[]
}

/**
 * Every seat at the table, in the order the server seated them.
 *
 * The order is `normalize.seatOrder`'s, because a seat's *name* depends on its position when
 * two seats share one, and a table drawn in one order while labelled from another would put the
 * qualifier on the wrong chair.
 */
export function seats(view: GameView): readonly Seat[] {
  const you = view.you ?? ''
  const opponents = new Map(list(view.opponents).map((opponent) => [opponent.player_id, opponent]))

  return seatOrder(view).map((id) => {
    const isYou = id === you
    const self = isYou ? view.me : undefined
    const opponent = opponents.get(id)
    const seat = self ?? opponent
    const piles = pilesFor(view, id)

    return {
      id,
      name: playerLabel(view, id),
      named: isNamed(view, id),
      isYou,
      life: seat?.life,
      librarySize: seat?.library_size,
      handSize: opponent?.hand_size,
      // The pile wins over the count. A graveyard is a public zone, so when the view itemizes
      // one, that list is what the player can see and count; `graveyard_size` is the summary
      // for a seat whose pile was not sent. Showing the summary over an itemized pile is how a
      // panel ends up reading `0 graveyard` above a card sitting in it.
      graveyardSize:
        piles.find((pile) => pile.zone === 'graveyard')?.faces.length ?? opponent?.graveyard_size,
      // A seat's statuses (the monarch, say) ride only on the opponent projection today.
      statuses: list(opponent?.statuses),
      eliminated: seat?.eliminated === true,
      connected: seat === undefined ? true : isConnected(seat),
      ai: seat?.ai === true,
      piles,
      manaPool: isYou ? list(view.mana_pool).map(manaPip) : [],
      commanderName: list(view.commander_identity).find((c) => c.commander === id)?.name,
      commanderTax: list(view.commander_tax).find((c) => c.commander === id)?.tax,
      commanderCasts: list(view.commander_tax).find((c) => c.commander === id)?.casts,
      commanderDamage: list(view.commander_damage)
        .filter((damage) => damage.damaged === id)
        .map((damage) => ({ from: damage.commander, amount: damage.amount })),
    }
  })
}

/** The public piles in front of one seat, in a fixed reading order, empty ones omitted. */
function pilesFor(view: GameView, player: string): readonly SeatPile[] {
  const zones = [
    { zone: 'graveyard', label: 'Graveyard', piles: list(view.graveyards) },
    { zone: 'exile', label: 'Exile', piles: list(view.exile) },
    { zone: 'command', label: 'Command', piles: list(view.command) },
  ] as const

  return zones.flatMap(({ zone, label, piles }) => {
    // Cards stay in the order the server listed them: a pile is ordered, and a graveyard's
    // order is information (CR 404.3) rather than a detail a client may re-sort.
    const cards = piles.filter((pile) => pile.player_id === player).flatMap((pile) => pile.cards)
    return cards.length === 0 ? [] : [{ zone, label, faces: cards.map(cardFace) }]
  })
}
