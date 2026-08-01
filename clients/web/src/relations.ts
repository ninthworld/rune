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
 */
import type { GameView } from './protocol'
import { list } from './normalize'

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

/** One line of an object's relationship trail: a phrase, and the entities it names. */
export interface RelationLine {
  kind: RelationKind
  /** Which end of the edge this object is on — what it states, or what is stated about it. */
  direction: 'from' | 'to'
  label: string
  /** The entities on the other end. Empty where the server named none. */
  ids: readonly string[]
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
 * Everything this object's relationships say about it, grouped for display.
 *
 * Ids rather than names: naming an entity is the screen's job, and returning ids is what lets a
 * caller draw each one as the control that reaches it. Two creatures blocking one attacker
 * become one line naming both, never two lines saying the same phrase twice.
 */
export function relationLines(all: Relations, id: string): readonly RelationLine[] {
  const outgoing = all.from(id)
  const incoming = all.to(id)

  return READING_ORDER.flatMap(({ kind, direction }) => {
    const matches = (direction === 'from' ? outgoing : incoming).filter(
      (relation) => relation.kind === kind,
    )
    if (matches.length === 0) return []
    const ids = matches
      .map((relation) => (direction === 'from' ? relation.to : relation.from))
      .filter((each): each is string => each !== undefined)
    return [{ kind, direction, label: WORDING[kind][direction], ids }]
  })
}
