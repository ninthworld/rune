/**
 * One seat's half of the battlefield.
 *
 * Two of these make the table, one per side, which is the whole point: a permanent's controller
 * is the single most-asked question on a board and it should be answered by where the card is,
 * not by reading a heading above a list.
 *
 * Permanents stay in the order the server listed them. There is no land row and no creature
 * row, because sorting them into one would mean deciding what a card *is* from its `type_line`
 * — a rules question the client does not get to answer. When the server projects presentation
 * categories, this is where they will land.
 *
 * Under each card hangs its relationship trail: what it is attacking, what blocked it, what it
 * is attached to, what is attached to it, what named it as a target, and what of its own is on
 * the stack. Those are relationships *between* objects rather than facts about one, so they are
 * joined in `relations.ts` from identifiers the server stated, and every name in the trail is a
 * control that reaches the object on the other end — which is often the only way to see it,
 * since the other end may be across the table or inside a pile.
 */
import type { Permanent } from './../../protocol'
import type { CardFace } from './../../card-face'
import type { RelationLine } from './../../relations'
import { Card } from './../Card'
import { RelationTrail } from './RelationTrail'
import type { Surface } from './surface'

export interface FieldEntry {
  permanent: Permanent
  face: CardFace
  lines: readonly RelationLine[]
}

export function Battlefield({
  entries,
  name,
  isYou,
  surface,
}: {
  entries: readonly FieldEntry[]
  name: string
  isYou: boolean
  surface: Surface
}) {
  return (
    <section
      className={`field ${isYou ? 'field--you' : 'field--opponent'}`}
      aria-label={isYou ? 'Your battlefield' : `${name} battlefield`}
    >
      {entries.length === 0 ? (
        <p className="field__empty">No permanents.</p>
      ) : (
        <ul className="cards cards--battlefield">
          {entries.map(({ permanent, face, lines }) => (
            <li key={permanent.id}>
              <Card
                face={face}
                variant="battlefield"
                state={surface.stateOf(face.id)}
                link={surface.linkOf(face.id)}
                onActivate={surface.activate}
                onTrace={surface.trace}
              />
              <RelationTrail lines={lines} surface={surface} />
            </li>
          ))}
        </ul>
      )}
    </section>
  )
}
