/**
 * What changed between the last two views — the only input any motion on this table is allowed
 * to have.
 *
 * An animation is a transition *between* two states the client could reconstruct on its own, and
 * never a third state of its own. So this is a pure function of two `GameView`s and it produces
 * facts, not effects: which objects the board is drawing that it was not drawing before, and by
 * how much a seat's life changed. Everything it returns is derived from ids and numbers the
 * **server** stated, and losing it — a refresh, a reconnect, a missed frame — costs a player
 * nothing except the transition itself.
 *
 * ## What cannot be animated yet, and why it is not attempted here
 *
 * A card flying from hand to the stack to the battlefield to a graveyard is the motion this
 * module was written for, and the wire does not currently support it. Entity ids live in three
 * separate spaces — `card_<instance>`, `perm_<permanent>`, `stack_<stack>` (`view/ids.rs`) — and
 * `Permanent.id` is documented as never colliding with a card's. A card played from hand is
 * therefore a *different id* in every zone it passes through, and nothing in the view links them.
 *
 * The only way a client could join them is by matching names or `functional_id`s, and that is
 * exactly the reasoning that does not live here: two Forests in a graveyard are indistinguishable
 * by both, so the client would be *deciding* which card moved. It would be wrong on precisely
 * the boards where it mattered — the ones with duplicates.
 *
 * So this module animates what the wire can support: an object **arriving** somewhere it was not,
 * which is a fact about one id in two consecutive views and needs no join at all. Moving a card
 * between two zones needs the server to say that two ids are one object, which is a protocol
 * change and a separate piece of work.
 */
import type { GameView } from './protocol'
import { list } from './normalize'

export interface Changes {
  /**
   * Ids this view draws that the previous one did not.
   *
   * A set rather than a list: this answers "did this object just appear", asked once per object
   * by whatever is drawing it, and the order objects appeared in is not something the view
   * states.
   */
  readonly arrived: ReadonlySet<string>
  /**
   * Seat id → the change in life since the previous view, negative for damage.
   *
   * Only seats both views stated a life total for. A seat that has just appeared has not *lost*
   * its whole life total, and one the view stopped carrying has not gone to zero — absence is a
   * fact about what the server said, and neither of those is a change a player watched happen.
   */
  readonly life: ReadonlyMap<string, number>
}

export const NO_CHANGES: Changes = { arrived: new Set(), life: new Map() }

/**
 * Compare two consecutive views.
 *
 * With no previous view there is no transition — the first frame of a game, and the first frame
 * after a reconnect, are states a player is arriving at rather than changes they are watching.
 * Animating them would flash every card on the board at once, which is the one moment a player
 * most needs to just read it.
 */
export function changes(previous: GameView | undefined, next: GameView): Changes {
  if (!previous) return NO_CHANGES

  const before = entities(previous)
  const arrived = new Set<string>()
  for (const id of entities(next)) if (!before.has(id)) arrived.add(id)

  const life = new Map<string, number>()
  const was = lifeTotals(previous)
  for (const [id, total] of lifeTotals(next)) {
    const previousTotal = was.get(id)
    if (previousTotal !== undefined && previousTotal !== total) life.set(id, total - previousTotal)
  }

  return { arrived, life }
}

/**
 * Every entity id one view draws.
 *
 * Piles included: a card put into a graveyard *arrives* in that graveyard under its own card id,
 * which is a real appearance and the one part of a zone change this client can state without
 * joining anything. The hidden zones are counts and have no ids to collect.
 */
function entities(view: GameView): ReadonlySet<string> {
  const ids = new Set<string>()
  const add = (id: string) => ids.add(id)

  for (const card of list(view.my_hand)) add(card.id)
  for (const permanent of list(view.battlefield)) add(permanent.id)
  for (const item of list(view.stack)) add(item.id)
  for (const emblem of list(view.emblems)) add(emblem.id)
  for (const card of list(view.revealed)) add(card.id)
  for (const pile of [...list(view.graveyards), ...list(view.exile), ...list(view.command)]) {
    for (const card of list(pile.cards)) add(card.id)
  }
  return ids
}

/** Life by seat id, from the two places the view states it: yours, and everyone else's. */
function lifeTotals(view: GameView): ReadonlyMap<string, number> {
  const totals = new Map<string, number>()
  const me = view.me
  if (me && view.you !== undefined) totals.set(view.you, me.life)
  for (const opponent of list(view.opponents)) totals.set(opponent.player_id, opponent.life)
  return totals
}

/**
 * How a life change reads, in words.
 *
 * The sign is the fact and the colour is the shortcut, never the other way round: a number with
 * no sign says nothing, and a red one says nothing to a player who cannot see red.
 */
export const lifeWording = (delta: number): string =>
  delta > 0 ? `gained ${delta} life` : `lost ${-delta} life`
