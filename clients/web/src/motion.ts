/**
 * What changed between the last two views — the only input any motion on this table is allowed
 * to have.
 *
 * An animation is a transition *between* two states the client could reconstruct on its own, and
 * never a third state of its own. So this is a pure function of two `GameView`s and it produces
 * facts, not effects: which objects the board is drawing that it was not drawing before, which
 * card moved from one zone to another, and by how much a seat's life changed. Everything it
 * returns is derived from ids and numbers the **server** stated, and losing it — a refresh, a
 * reconnect, a missed frame — costs a player nothing except the transition itself.
 *
 * ## Following a card is the server's join, never this module's guess
 *
 * A card played from hand is a *different object* in every zone it passes through — `card_`,
 * `perm_`, `stack_` are three id spaces, and under CR 400.7 that is the rule rather than a gap:
 * an object that changes zone becomes a new object with no relation to its previous existence.
 * What persists is the **physical card** (CR 108.1), and the server states it: `physical_card` on
 * a permanent and on a spell, carrying the same id that card has as its own wherever a view shows
 * it in a zone.
 *
 * So the join here is by that id and by nothing else. Matching on `name` or `functional_id` is a
 * bug, not a fallback: two Forests agree on both, so a name join is the client *deciding* which
 * one moved and being wrong on exactly the boards where it mattered.
 *
 * And a flight says nothing about identity. The two ends are two objects; that the same card is
 * behind them is all this claims, and nothing downstream may carry counters, damage, attachments,
 * or control across the gap — the rules just discarded all of it.
 */
import type { GameView } from './protocol'
import { list } from './normalize'

/**
 * One card drawn in two places by two consecutive views: where it was, and where it is.
 *
 * `from` and `to` are *object* ids in their own view — a hand card's, a stack object's, a
 * permanent's — and they are deliberately different objects. `card` is the physical card both are
 * projections of, and is the only thing that makes them a pair.
 */
export interface Flight {
  /** The id the previous view drew this card under. */
  readonly from: string
  /** The id this view draws it under. */
  readonly to: string
  /** The physical card (CR 108.1) both are projections of — the join, and nothing more. */
  readonly card: string
}

export interface Changes {
  /**
   * Ids this view draws that the previous one did not, **excluding** anything that flew here.
   *
   * A set rather than a list: this answers "did this object just appear", asked once per object
   * by whatever is drawing it, and the order objects appeared in is not something the view
   * states. A card that moved zones is a different id in the new view and so would qualify twice
   * over — it is left out here, because arriving *and* flying is one event drawn as two.
   */
  readonly arrived: ReadonlySet<string>
  /**
   * Cards this view draws somewhere the previous one drew them too, under a different id.
   *
   * Ordered only for determinism; the order is the new view's, and it carries no meaning.
   */
  readonly flights: readonly Flight[]
  /**
   * Seat id → the change in life since the previous view, negative for damage.
   *
   * Only seats both views stated a life total for. A seat that has just appeared has not *lost*
   * its whole life total, and one the view stopped carrying has not gone to zero — absence is a
   * fact about what the server said, and neither of those is a change a player watched happen.
   */
  readonly life: ReadonlyMap<string, number>
}

export const NO_CHANGES: Changes = { arrived: new Set(), flights: [], life: new Map() }

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
  const wasAt = whereCardsAre(previous)
  const isAt = whereCardsAre(next)

  const flights: Flight[] = []
  for (const [card, to] of isAt) {
    const from = wasAt.get(card)
    if (from !== undefined && from !== to) flights.push({ from, to, card })
  }

  const flown = new Set(flights.map((flight) => flight.to))
  const arrived = new Set<string>()
  for (const id of entities(next)) if (!before.has(id) && !flown.has(id)) arrived.add(id)

  const life = new Map<string, number>()
  const was = lifeTotals(previous)
  for (const [id, total] of lifeTotals(next)) {
    const previousTotal = was.get(id)
    if (previousTotal !== undefined && previousTotal !== total) life.set(id, total - previousTotal)
  }

  return { arrived, flights, life }
}

/**
 * Every entity id one view draws.
 *
 * Piles included: a card put into a graveyard *arrives* in that graveyard under its own card id,
 * which is a real appearance. The hidden zones are counts and have no ids to collect.
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

/**
 * Physical card → the id one view draws it under.
 *
 * Three shapes feed it, and all three are the server's own statement. A card in a zone *is* its
 * physical card's id, which is why no new id space was needed for any of this. A permanent and a
 * spell each name theirs; an ability and a token name none, and so are simply not here — an
 * ability has no card, and a token is not one (CR 111).
 *
 * A card the view somehow draws in two places at once is dropped rather than guessed at. Which of
 * the two an animation should fly from is not something the view states, and picking one would be
 * the client deciding — the same mistake as joining by name, arrived at more quietly.
 */
function whereCardsAre(view: GameView): ReadonlyMap<string, string> {
  const at = new Map<string, string>()
  const ambiguous = new Set<string>()
  const place = (card: string, entity: string) => {
    const already = at.get(card)
    if (already === undefined) at.set(card, entity)
    else if (already !== entity) ambiguous.add(card)
  }

  for (const card of list(view.my_hand)) place(card.id, card.id)
  for (const card of list(view.revealed)) place(card.id, card.id)
  for (const pile of [...list(view.graveyards), ...list(view.exile), ...list(view.command)]) {
    for (const card of list(pile.cards)) place(card.id, card.id)
  }
  for (const permanent of list(view.battlefield)) {
    if (permanent.physical_card !== undefined) place(permanent.physical_card, permanent.id)
  }
  for (const item of list(view.stack)) {
    if (item.physical_card !== undefined) place(item.physical_card, item.id)
  }

  for (const card of ambiguous) at.delete(card)
  return at
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
