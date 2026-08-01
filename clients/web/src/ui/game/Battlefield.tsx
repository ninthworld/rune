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
 */
import type { Permanent } from './../../protocol'
import type { CardFace } from './../../card-face'
import { Card } from './../Card'
import type { Surface } from './surface'

export interface FieldEntry {
  permanent: Permanent
  face: CardFace
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
          {entries.map(({ permanent, face }) => (
            <li key={permanent.id}>
              <Card
                face={face}
                variant="battlefield"
                state={surface.stateOf(face.id)}
                onActivate={surface.activate}
              />
              {/* Combat and attachment are relationships *between* objects rather than facts
                  about one, so they stay beside the face as text until the table can draw the
                  line itself (#627). */}
              {(permanent.attacking || permanent.blocking || permanent.attached_to) && (
                <p className="cards__aside">
                  {permanent.attacking &&
                    (permanent.attacking_planeswalker !== undefined
                      ? `attacking ${surface.labelFor(permanent.attacking_planeswalker)}`
                      : 'attacking')}
                  {permanent.blocking && ` blocking ${surface.labelFor(permanent.blocking)}`}
                  {permanent.attached_to &&
                    ` attached to ${surface.labelFor(permanent.attached_to)}`}
                </p>
              )}
            </li>
          ))}
        </ul>
      )}
    </section>
  )
}
