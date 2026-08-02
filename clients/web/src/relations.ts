/**
 * What points at what: the relationships the server projected, joined once and readable both ways.
 *
 * A board is not a set of independent objects. An attacker means nothing without the seat or
 * planeswalker it is attacking, a blocker means nothing without the attacker it stopped, and a
 * spell on the stack means nothing without what it named. Those facts arrive scattered across
 * the view — `attacking_player` and `attacking_planeswalker` and `blocking` and `attached_to` on
 * a `Permanent`, `targets` and `source` on a `StackItem` — and every one of them is stated in
 * one direction only.
 *
 * One direction is not enough to draw a board. A blocker knows what it blocks; the attacker does
 * not know what blocked it, and "how many creatures blocked this one" is the question a player
 * actually asks in combat. So the edges are indexed from both ends here, once, and the surfaces
 * ask this module rather than each scanning the battlefield for the reverse.
 *
 * **Every edge is an identifier the server stated.** Nothing here reads rules text, parses a log
 * line, or concludes that two objects must be related because they look like they should be. An
 * attacker whose defender the server did not name renders as attacking *something unnamed*
 * (`to` absent), because that is exactly what the view said — the alternative is guessing at a
 * defender, which is the client deciding a fact about the game.
 *
 * An identifier is not a name, which is why `entityNames` is here too: the trail under a card is
 * read by a player, and `perm_vivien` is for a log (`docs/client-design.md` §9.2). The join from
 * an id to the name the view stated for it is the text half of what `overlay.ts` already does for
 * the picture half — an edge whose other end the client cannot show clips to nothing rather than
 * pointing confidently at something nobody can see.
 */
import type { GameLogEvent, GameView, LogEntity } from './protocol'
import { list, playerLabel } from './normalize'

/**
 * The kinds of relationship a view projects.
 *
 * Each is named from the perspective of the object that *carries* the field, which is the
 * direction the wire states it in: a blocker carries `blocking`, so the edge runs blocker →
 * attacker and the attacker's side of it is derived.
 */
export type RelationKind = 'attacking' | 'blocking' | 'attached' | 'targeting' | 'source'

/** One stated relationship, from the object that carries the field to the object it names. */
export interface Relation {
  kind: RelationKind
  /** The entity that carries the projected field. */
  from: string
  /**
   * The entity or seat that field names. Absent where the server stated the relationship
   * without naming its other end — an `attacking` permanent with no defender projected.
   */
  to?: string
}

/** The relationships in one view, indexed from both ends. */
export interface Relations {
  /** Every relation, in the order the view listed the objects that carry them. */
  readonly all: readonly Relation[]
  /** What this object states about others. */
  from(id: string): readonly Relation[]
  /** What others state about this object. */
  to(id: string): readonly Relation[]
  /** Every entity on the other end of a relation touching this one, in either direction. */
  linked(id: string): ReadonlySet<string>
}

/**
 * Join a view's relationships.
 *
 * Order matters and is the view's own: combat is read off the battlefield in the order the
 * server listed the permanents, and targets in the order it listed the stack. A player reading
 * "blocked by Grizzly Bears, Thopter" is reading the server's ordering of the board, not this
 * client's idea of which blocker matters more.
 */
export function relations(view: GameView): Relations {
  const all: Relation[] = []

  for (const permanent of list(view.battlefield)) {
    if (permanent.attacking === true) {
      // The planeswalker wins over the seat: when a permanent is attacking a planeswalker the
      // server sends both, `attacking_player` being the seat that answers for the attack
      // (`docs/protocol.md`). Naming the seat there would say the wrong thing about where the
      // damage is going, and naming both would draw one attack as two.
      all.push({
        kind: 'attacking',
        from: permanent.id,
        to: permanent.attacking_planeswalker ?? permanent.attacking_player,
      })
    }
    if (permanent.blocking !== undefined) {
      all.push({ kind: 'blocking', from: permanent.id, to: permanent.blocking })
    }
    if (permanent.attached_to !== undefined) {
      all.push({ kind: 'attached', from: permanent.id, to: permanent.attached_to })
    }
  }

  for (const item of list(view.stack)) {
    if (item.source !== undefined) {
      all.push({ kind: 'source', from: item.id, to: item.source })
    }
    for (const target of list(item.targets)) {
      // A target is a tagged union of four things, and all four are addressed by the same
      // entity id everywhere else on the screen — a seat included, which is why a burn spell
      // aimed at a player traces to that player's panel exactly as one aimed at a creature
      // traces to its card.
      all.push({
        kind: 'targeting',
        from: item.id,
        to: 'id' in target ? target.id : target.player,
      })
    }
  }

  const outgoing = index(all, (relation) => relation.from)
  const incoming = index(all, (relation) => relation.to)

  return {
    all,
    from: (id) => outgoing.get(id) ?? [],
    to: (id) => incoming.get(id) ?? [],
    linked: (id) => {
      const ends = new Set<string>()
      for (const relation of outgoing.get(id) ?? [])
        if (relation.to !== undefined) ends.add(relation.to)
      for (const relation of incoming.get(id) ?? []) ends.add(relation.from)
      return ends
    },
  }
}

function index(
  all: readonly Relation[],
  key: (relation: Relation) => string | undefined,
): ReadonlyMap<string, Relation[]> {
  const map = new Map<string, Relation[]>()
  for (const relation of all) {
    const id = key(relation)
    if (id === undefined) continue
    const bucket = map.get(id)
    if (bucket) bucket.push(relation)
    else map.set(id, [relation])
  }
  return map
}

/**
 * What a control says about an object this view never described.
 *
 * Not for the trail — an end with no name is simply not drawn there. This is for the controls
 * that have to exist whatever happens: the dock's fallback button for a subject no surface drew
 * (`dock.ts`), and the heading over an object's own actions. A control a player must be able to
 * press cannot be a blank, and it cannot be the id either, so it says the one true thing left.
 * It states no kind, because the kind of an object the view did not describe is not something
 * this client is allowed to work out.
 */
export const UNNAMED = 'Unnamed object'

/** The word an emblem is drawn under everywhere, including `card-face.ts`. Emblems have no name. */
const EMBLEM = 'Emblem'

/**
 * Every name this view states for an entity id, joined once.
 *
 * A relationship arrives as an identifier, and the surface that draws it needs a word. That word
 * is only ever one the **view** stated: this module reads no card database, resolves nothing
 * against the catalog, and never turns an id into a name by looking at the id. An entity the view
 * did not describe anywhere simply has no entry, and the surfaces treat that absence as an
 * absence — which is the whole point, because the fallback that used to fill it was the id.
 *
 * The join is total over the view's own name-bearing shapes, and being total is what makes it
 * worth doing: a version that covered the battlefield and not the graveyard would move the defect
 * rather than fix it. Every one of these is a name the server put on the wire:
 *
 * - every `CardView` anywhere — `my_hand`, `revealed`, `graveyards`, `exile`, `command`, the card
 *   on a `Permanent`, and the card on a `StackItem` — names its own id;
 * - a `Permanent` is named by its card, under the permanent's id, which is the id every
 *   relationship addresses it by;
 * - a `StackItem` is named by its server-composed `description`, because "Counterspell targeting
 *   Twin Bolt" is what the object on the stack *is* and its card's name is only half of it;
 * - an `Emblem` has no name at all (CR 114), so it gets the same word the board draws it under;
 * - a seat is named the way every seat is named on this table (`playerLabel`), so the trail and
 *   the panel across from it cannot call one player two things;
 * - the `log` states `{id, name}` pairs, and an object that has left every visible zone is often
 *   named there and nowhere else.
 *
 * Precedence runs lowest first, so a live projection always beats a historical one: the log is
 * laid down before anything the view currently shows, and a card carried alongside a stack object
 * is laid down before the battlefield, which is the only other place that id can appear.
 *
 * An empty name is not a name. A blank on the wire is stated absence, not a label, and letting it
 * through would put a control with nothing in it under a card.
 */
export function entityNames(view: GameView): ReadonlyMap<string, string> {
  const names = new Map<string, string>()
  const state = (id: string | undefined, name: string | undefined) => {
    const stated = name?.trim()
    if (id === undefined || !stated) return
    names.set(id, stated)
  }

  for (const entry of list(view.log))
    for (const entity of logEntities(entry.event)) state(entity.id, entity.name)

  for (const pile of [...list(view.graveyards), ...list(view.exile), ...list(view.command)])
    for (const card of list(pile.cards)) state(card.id, card.name)

  for (const card of [...list(view.my_hand), ...list(view.revealed)]) state(card.id, card.name)

  // Before the battlefield: an ability's `card` is the *source permanent's* face, keyed by that
  // permanent's id (`docs/protocol.md`), and where the two disagree the board is the live one.
  for (const item of list(view.stack)) state(item.card?.id, item.card?.name)

  for (const permanent of list(view.battlefield)) {
    state(permanent.card.id, permanent.card.name)
    state(permanent.id, permanent.card.name)
  }

  for (const item of list(view.stack)) state(item.id, item.description)

  for (const emblem of list(view.emblems)) state(emblem.id, EMBLEM)

  // Deliberately last and deliberately unconditional. `playerLabel` falls back to the seat's own
  // id when the server named nobody, and that is a name here rather than a leaked identifier:
  // it is the same string the seat's panel, its battlefield region, and its life total are all
  // labelled with, so a trail that dropped it would be hiding a seat the player can see.
  for (const seat of seatIds(view)) state(seat, playerLabel(view, seat))

  return names
}

/** Every seat the view mentions, however it mentioned it. Duplicates are the map's problem. */
function seatIds(view: GameView): readonly string[] {
  return [
    ...(view.you === undefined ? [] : [view.you]),
    ...list(view.seat_order),
    ...list(view.opponents).map((opponent) => opponent.player_id),
    ...Object.keys(view.player_names ?? {}),
  ]
}

/**
 * The `{id, name}` pairs one log event states.
 *
 * The log is a name source and nothing more here — no event is read for what *happened*, which
 * would be this client reconstructing the game from its transcript. `LogEntity` is the server
 * saying "this id is called that", which is exactly the question being asked.
 */
function logEntities(event: GameLogEvent): readonly LogEntity[] {
  switch (event.type) {
    case 'spell_cast':
    case 'spell_resolved':
    case 'spell_countered':
    case 'spell_fizzled':
    case 'commander_returned_to_command_zone':
      return [event.card]
    case 'attackers_declared':
      return event.attackers
    case 'blockers_declared':
      return event.blocks.flatMap((block) => [block.blocker, block.attacker])
    case 'permanent_died':
      return [event.permanent]
    case 'damage_dealt':
      return event.target.kind === 'permanent' ? [event.target.permanent] : []
    default:
      return []
  }
}

/**
 * The wording for one relationship, in each direction.
 *
 * Both halves are written here rather than at the two call sites, so the attacker's "blocked by"
 * and the blocker's "blocking" can never end up describing the same edge as two different
 * events. The phrasing is deliberately plain and states no rule: `attached` says a thing is
 * attached, not that it is an Aura enchanting a creature or an Equipment equipping one, because
 * which of those it is is a question about the card and the client does not read cards.
 */
const WORDING: Record<RelationKind, { from: string; to: string }> = {
  attacking: { from: 'attacking', to: 'attacked by' },
  blocking: { from: 'blocking', to: 'blocked by' },
  attached: { from: 'attached to', to: 'attached' },
  targeting: { from: 'targeting', to: 'targeted by' },
  source: { from: 'from', to: 'on the stack' },
}

/**
 * One object on the other end of a relationship, ready to be drawn.
 *
 * Both halves ride together on purpose. The id is what a control on it *reaches* and never what
 * it *says*; the name is what it says and is never how anything is addressed. Keeping them in one
 * value is what makes it impossible for a surface to hold an end it cannot name — the case that
 * used to print `perm_vivien` under a card.
 */
export interface RelationEnd {
  /** The identifier the server stated. Routed through `interaction.ts`, never rendered. */
  id: string
  /** The name this view stated for it (`entityNames`). Never an identifier. */
  name: string
}

/** One line of an object's relationship trail: a phrase, and the entities it names. */
export interface RelationLine {
  kind: RelationKind
  /** Which end of the edge this object is on — what it states, or what is stated about it. */
  direction: 'from' | 'to'
  label: string
  /** The objects on the other end. Empty where this view names none of them. */
  ends: readonly RelationEnd[]
}

/**
 * A fixed reading order, so one object's trail does not reshuffle as the board changes.
 *
 * Combat first and in the order it is declared, then attachments, then the stack — which is the
 * order a player scans a permanent in, and it is the same order for every object so the second
 * line always means the same thing as it did on the last one.
 */
const READING_ORDER: readonly { kind: RelationKind; direction: 'from' | 'to' }[] = [
  { kind: 'attacking', direction: 'from' },
  { kind: 'attacking', direction: 'to' },
  { kind: 'blocking', direction: 'from' },
  { kind: 'blocking', direction: 'to' },
  { kind: 'attached', direction: 'from' },
  { kind: 'attached', direction: 'to' },
  { kind: 'targeting', direction: 'from' },
  { kind: 'targeting', direction: 'to' },
  { kind: 'source', direction: 'from' },
  { kind: 'source', direction: 'to' },
]

/**
 * Everything this object's relationships say about it, grouped for display, and named.
 *
 * The naming happens *here* rather than at the surface, and that is the point: a component that
 * received bare ids could print one, and one of them did. What comes out is a phrase and a list of
 * ends that each carry both an id to reach and a word to say, so there is no path from an
 * identifier to anything a player reads. Two creatures blocking one attacker become one line
 * naming both, never two lines saying the same phrase twice.
 *
 * **An end this view does not name is dropped, and the phrase stays.** Two cases arrive here
 * looking identical and are answered identically, because they are the same fact: the server
 * stated a relationship and this view does not describe its other end. One is an attacker with no
 * defender projected (`to` absent); the other is a defender the view names nowhere — a permanent
 * that left the battlefield, an object in a zone this seat cannot see. Neither leaves anything to
 * click through to, and neither may be filled in: the only word available would be one this client
 * worked out from the *relationship* — "a creature", because something blocked it — and concluding
 * a fact about an object from an edge pointing at it is the client deciding the game. So the line
 * reads `attacking`, which is exactly what the view said and no more, and the drawn overlay does
 * the same thing for the same reason (`overlay.ts`).
 */
export function relationLines(
  all: Relations,
  id: string,
  names: ReadonlyMap<string, string>,
): readonly RelationLine[] {
  const outgoing = all.from(id)
  const incoming = all.to(id)

  return READING_ORDER.flatMap(({ kind, direction }) => {
    const matches = (direction === 'from' ? outgoing : incoming).filter(
      (relation) => relation.kind === kind,
    )
    if (matches.length === 0) return []
    const ends = matches.flatMap((relation) => {
      const end = direction === 'from' ? relation.to : relation.from
      if (end === undefined) return []
      const name = names.get(end)
      return name === undefined ? [] : [{ id: end, name }]
    })
    return [{ kind, direction, label: WORDING[kind][direction], ends }]
  })
}
