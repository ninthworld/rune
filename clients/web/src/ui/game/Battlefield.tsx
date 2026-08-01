/**
 * One seat's half of the battlefield.
 *
 * Two of these make the table, one per side, which is the whole point: a permanent's controller
 * is the single most-asked question on a board and it should be answered by where the card is,
 * not by reading a heading above a list.
 *
 * Inside a half, permanents are grouped into rows — creatures, other permanents, lands — by
 * `board.ts`, from the types the **server** stated (`CardView.card_types`). The client parses no
 * type line to get there and decides nothing about what a card is; what it decides is where to
 * draw a permanent that is more than one thing, which is presentation with no rules content.
 * Within a row the server's own order is kept, because that is the only order anything here is
 * entitled to.
 *
 * The two halves mirror, so both sets of creatures meet at the dividing line and combat reads as
 * one band rather than two lists that happen to be stacked.
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
import { boardRows } from './../../board'
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
  const rows = boardRows(entries, (entry) => entry.face.cardTypes, { mirrored: !isYou })

  return (
    <section
      className={`field ${isYou ? 'field--you' : 'field--opponent'}`}
      aria-label={isYou ? 'Your battlefield' : `${name} battlefield`}
    >
      {entries.length === 0 ? (
        <p className="field__empty">No permanents.</p>
      ) : (
        rows.map((group) => (
          // Named for a screen reader, which cannot see that a row is a row. Sighted readers
          // get the grouping itself and no heading — a board with three labels stacked down it
          // spends its scarcest resource, vertical space, on words a player already knows.
          <ul
            key={group.row}
            className={`cards cards--battlefield field__row field__row--${group.row}`}
            aria-label={isYou ? group.label : `${name} ${group.label.toLowerCase()}`}
          >
            {group.entries.map(({ permanent, face, lines }) => (
              <li key={permanent.id}>
                {/* The slot, not the card, is what reserves room for a tapped permanent. A card
                    that turns 90° is wider than the box it stood in, so the slot widens over the
                    same half second the card takes to turn and the neighbours slide aside —
                    which is what happens when a real card is laid down. Without it a tapped
                    permanent would overlap the one beside it. */}
                <span className="card-slot">
                  <Card
                    face={face}
                    state={surface.stateOf(face.id)}
                    link={surface.linkOf(face.id)}
                    onActivate={surface.activate}
                    onInspect={surface.inspect}
                    onTrace={surface.trace}
                  />
                </span>
                <RelationTrail lines={lines} surface={surface} />
              </li>
            ))}
          </ul>
        ))
      )}
    </section>
  )
}
