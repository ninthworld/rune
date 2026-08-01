/**
 * One browsable pile of cards, wherever the cards came from.
 *
 * A graveyard, an exile pile, a command zone, and the cards a choice is showing you are four
 * different things in the rules and one thing on screen: an ordered list of faces a player wants
 * to read and sometimes act on. They get one surface, so a graveyard cannot end up with a
 * different affordance than the pile a search prompt is asking about.
 *
 * Cards keep the order the server listed them in. A graveyard's order is information (CR 404.3),
 * not a detail to re-sort, and a choice set's order is the order the prompt's candidates were
 * offered in.
 *
 * The panel opens beside the table rather than over it. A pile is often the thing a player is
 * choosing *from* while the dock asks the question, so covering the board or the controls with
 * it would make the two halves of one decision take turns.
 *
 * **The cards are cards.** A pile used to be a column of one-line summaries, which is the right
 * shape for a list of names and the wrong shape for the question a player actually opens a
 * graveyard to ask — what is *in* here, scanned by art and frame the way a board is scanned. It
 * draws the same face the battlefield draws, so a creature in a graveyard looks like the creature
 * it was, and the column it opens in widens to make room for them rather than shrinking the cards
 * to fit a rail. The board gives up that width only while a pile is open.
 */
import type { CardFace } from './../../card-face'
import { Card } from './../Card'
import type { Surface } from './surface'

export function ZonePanel({
  label,
  note,
  faces,
  surface,
  onClose,
}: {
  /** The accessible name of the region, kept stable while the count beside it changes. */
  label: string
  /** Whose pile this is, or why it is being shown. Omitted where the label says it already. */
  note?: string
  faces: readonly CardFace[]
  surface: Surface
  /** Absent for a panel the player did not open and cannot dismiss. */
  onClose?(): void
}) {
  return (
    <section className="side__block zone" aria-label={label}>
      <div className="zone__head">
        <h2>
          {label} ({faces.length})
        </h2>
        {onClose && (
          <button type="button" className="zone__close" onClick={onClose}>
            Close
          </button>
        )}
      </div>

      {note && <p className="zone__note">{note}</p>}

      {faces.length === 0 ? (
        // Reachable: a pile can empty while it is open, and saying so is a better answer than
        // closing the panel out from under the player who was reading it.
        <p className="zone__empty">Empty.</p>
      ) : (
        <ul className="cards cards--pile">
          {faces.map((face) => (
            <li key={face.id}>
              <Card
                face={face}
                variant="battlefield"
                state={surface.stateOf(face.id)}
                link={surface.linkOf(face.id)}
                onActivate={surface.activate}
                onInspect={surface.inspect}
                onTrace={surface.trace}
              />
            </li>
          ))}
        </ul>
      )}
    </section>
  )
}
