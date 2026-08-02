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
 * **The box is the scene's and the count is `pack.ts`'s problem.** A battlefield is handed a
 * rectangle and never asks for one: fifteen permanents in a field sized for six do not make the
 * field taller, they make the cards smaller and then overlapped and then chips (§5). Both halves
 * are therefore always the same height and the line across the middle of the table does not move
 * for any game event — a seat that wipes the opponent's board does not watch its own permanents
 * jump to a new size and a new place. Every tile below is placed at a computed point rather than
 * flowed, which is what makes that true whatever is in the row: nothing here can grow a box, and
 * there is no overflow for a scrollbar to appear in.
 *
 * **The relationship trail is no longer drawn under the card.** It was what set a permanent's
 * column width — 233px of `blocked by Colossal Dreadmaw Zombie` around a 108px card — and a
 * geometry decided by a sentence about an object is exactly what §5 removes. The words stay in
 * the document, where assistive technology reads them and where they remain the accessible
 * equivalent of the line `RelationOverlay` draws across the board; what they no longer do is
 * decide how big a card is. The seat bar and the stack rail still draw their trails, because
 * neither of them is packing a row.
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
import { boardRows, type BoardGroup } from './../../board'
import { packField, type Box } from './../../pack'
import { Card } from './../Card'
import { RelationTrail } from './RelationTrail'
import type { Surface } from './surface'

export interface FieldEntry {
  permanent: Permanent
  face: CardFace
  lines: readonly RelationLine[]
}

/** One row as this field will draw it: a `board.ts` group, or the row several of them merged into. */
interface DrawnRow {
  key: string
  label: string
  entries: readonly FieldEntry[]
}

/**
 * Give up the split, as far as the table's row count requires.
 *
 * `pack.ts` answers how many rows the *table* draws, and it answers it to maximise card size
 * rather than row count (§3, "More screen is never a worse board"). A half with more groups than
 * that has to merge some of them, and **which** ones is the only decision here: the rows furthest
 * from the dividing line go first, so the creature band the two halves meet across is the last
 * thing to lose its own row. Combat is read along that line, and a board that merged the creatures
 * into the lands to keep an artifact row would have spent the wrong one.
 *
 * The grouping is what is given up — never the ordering, and never which row a permanent belongs
 * to. Entries stay in `board.ts`'s order inside the merged row exactly as they were in their own.
 * A mirrored half draws its groups reversed, so the far end of it is the front of the array.
 */
function collapse(
  groups: readonly BoardGroup<FieldEntry>[],
  slots: number,
  mirrored: boolean,
): readonly DrawnRow[] {
  const rows = Math.max(1, Math.floor(slots))
  const own = (group: BoardGroup<FieldEntry>): DrawnRow => ({
    key: group.row,
    label: group.label,
    entries: group.entries,
  })
  if (groups.length <= rows) return groups.map(own)
  if (rows === 1) {
    return [{ key: 'merged', label: 'Permanents', entries: groups.flatMap((row) => row.entries) }]
  }

  const merging = groups.length - rows + 1
  const gone = mirrored ? groups.slice(0, merging) : groups.slice(rows - 1)
  const kept = mirrored ? groups.slice(merging) : groups.slice(0, rows - 1)
  // Named for what is in it, in the order it is drawn — a screen reader is told the row holds
  // other permanents and lands, because that is what the row is once the split is given up.
  const merged: DrawnRow = {
    key: 'merged',
    label: gone
      .map((row, index) => (index === 0 ? row.label : row.label.toLowerCase()))
      .join(' and '),
    entries: gone.flatMap((row) => row.entries),
  }
  return mirrored ? [merged, ...kept.map(own)] : [...kept.map(own), merged]
}

export function Battlefield({
  entries,
  name,
  isYou,
  box,
  slots = 1,
  cardTier = 'compact',
  surface,
}: {
  entries: readonly FieldEntry[]
  name: string
  isYou: boolean
  /** The region the scene gave this field. Nothing here ever asks for a different one. */
  box: Box
  /**
   * How many rows the *table* draws (§3, step 5), which `pack.ts` decides for both halves at
   * once. One means this field's groups are drawn as a single row.
   */
  slots?: number
  /** §6's presentations, as the room allows — the ladder's steps 2–4, arriving as one word. */
  cardTier?: SceneLadder['cardTier']
  surface: Surface
}) {
  const groups = boardRows(entries, (entry) => entry.face.cardTypes, { mirrored: !isYou })
  const drawn = collapse(groups, slots, !isYou)
  const plan = packField(
    box,
    drawn.map((group) => group.entries.length),
    { slots, mirrored: !isYou, cardTier },
  )
  const tier = plan.rows[0]?.pack.tier ?? cardTier

  return (
    <section
      className={[
        'field',
        isYou ? 'field--you' : 'field--opponent',
        `field--rows-${slots === 1 ? 'merged' : 'split'}`,
        `field--tier-${tier}`,
      ].join(' ')}
      aria-label={isYou ? 'Your battlefield' : `${name} battlefield`}
    >
      {plan.rows.map((row, index) => {
        const group = drawn[index]
        if (!group) return null
        return (
          // Named for a screen reader, which cannot see that a row is a row. Sighted readers
          // get the grouping itself and no heading — a board with three labels stacked down it
          // spends its scarcest resource, vertical space, on words a player already knows.
          <ul
            key={group.key}
            className={`cards cards--battlefield field__row field__row--${group.key}`}
            aria-label={isYou ? group.label : `${name} ${group.label.toLowerCase()}`}
            style={{ left: row.x, top: row.y, width: row.width, height: row.height }}
          >
            {group.entries.map(({ permanent, face, lines }, position) => (
              // One box per permanent, whatever its state. A tapped permanent used to turn a
              // quarter and needed a slot around it to reserve the landscape footprint; it is
              // marked upright now (§6), so every tile on the row is the same card-shaped box
              // and a row has one packing problem instead of two.
              //
              // Later tiles lie over earlier ones, so what an overlapped row leaves showing is
              // each card's top-left corner: the name band (§3, step 4).
              <li
                key={permanent.id}
                style={{
                  left: row.pack.positions[position]?.x ?? 0,
                  top: row.pack.positions[position]?.y ?? 0,
                  width: row.pack.width,
                  height: row.pack.height,
                  zIndex: position,
                }}
              >
                <Card
                  face={face}
                  state={surface.stateOf(face.id)}
                  link={surface.linkOf(face.id)}
                  onActivate={surface.activate}
                  onInspect={surface.inspect}
                  onTrace={surface.trace}
                />
                {/* The same trail, in the document only. Every relationship it names is drawn
                    across the board as a line, and a line is not readable — so this is the copy
                    a screen reader gets and the traversal a keyboard keeps, and it is out of the
                    packing entirely because a card's size may not be decided by a sentence. */}
                <div className="visually-hidden">
                  <RelationTrail lines={lines} surface={surface} />
                </div>
              </li>
            ))}
          </ul>
        )
      })}
    </section>
  )
}
