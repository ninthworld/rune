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
 *
 * **The box is the scene's and the count is this field's problem.** A battlefield is handed a
 * height by `scene.ts` and never asks for one: fifteen permanents in a field sized for six do not
 * make the field taller, they make the cards smaller and then overlapped (§5). Both halves are
 * therefore always the same height and the line across the middle of the table does not move for
 * any game event — a seat that wipes the opponent's board does not watch its own permanents jump
 * to a new size and a new place. `rows` and `cardTier` are how the ladder reaches the packing
 * that does the absorbing; today they are stated on the element and the packing is #660's.
 *
 * There is no "No permanents." sentence, and its absence is the point. It spent a card row's
 * height printing a fact the empty half of the table already states, and "my opponent has
 * nothing" is read by *looking at an empty place* rather than by reading a label about one. The
 * place stays; the sentence went.
 */
import type { Permanent } from './../../protocol'
import type { CardFace } from './../../card-face'
import type { RelationLine } from './../../relations'
import type { SceneLadder } from './../../scene'
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
  rows = 'split',
  cardTier = 'compact',
  surface,
}: {
  entries: readonly FieldEntry[]
  name: string
  isYou: boolean
  /** §3, step 5: whether this field's height buys two rows of card faces or one taller one. */
  rows?: SceneLadder['rows']
  /** §6's presentations, as the room allows — the ladder's steps 2–4, arriving as one word. */
  cardTier?: SceneLadder['cardTier']
  surface: Surface
}) {
  const groups = boardRows(entries, (entry) => entry.face.cardTypes, { mirrored: !isYou })

  return (
    <section
      className={[
        'field',
        isYou ? 'field--you' : 'field--opponent',
        `field--rows-${rows}`,
        `field--tier-${cardTier}`,
      ].join(' ')}
      aria-label={isYou ? 'Your battlefield' : `${name} battlefield`}
    >
      {groups.map((group) => (
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
              {/* One box per permanent, whatever its state. A tapped permanent used to turn a
                  quarter and needed a slot around it to reserve the landscape footprint; it is
                  marked upright now (§6), so every tile on the row is the same card-shaped
                  box and a row has one packing problem instead of two. */}
              <Card
                face={face}
                state={surface.stateOf(face.id)}
                link={surface.linkOf(face.id)}
                onActivate={surface.activate}
                onInspect={surface.inspect}
                onTrace={surface.trace}
              />
              <RelationTrail lines={lines} surface={surface} />
            </li>
          ))}
        </ul>
      ))}
    </section>
  )
}
