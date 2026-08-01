/**
 * Your hand, along the bottom edge.
 *
 * The one zone only you can see, so it gets the edge nearest you — and, where there is room, a
 * full row to itself. Its box is the scene's like every other region's: a hand of twenty cards
 * does not make the band taller, because the band is what the viewport can afford and the count
 * is the cards' problem (§5).
 *
 * **The peek strip is the other half of §2's trade.** On a phone the hand and the action
 * affordance both want the bottom band and cannot both have it, so they take turns: the hand is
 * full height while the game is asking nothing, and the moment there is something to answer it
 * collapses to a strip and the dock takes the height it freed. This is a change of *mode* and not
 * a measure of content — which is exactly why §5 allows it where an empty battlefield gets no
 * such licence.
 *
 * A peeked card is a landscape chip rather than a shrunken card: below the card floor a frame is
 * not a smaller card, it is an unreadable one (§3, step 6), and a chip still carries the name —
 * which in the hand may never be abbreviated at all, because the hand is where a player chooses.
 * So the strip is small, and never invisible.
 *
 * Raising it is one gesture, and what comes up is an *overlay* rather than a taller region: the
 * height a full hand needs belongs to tier-1 minimums the board cannot give up while something is
 * being asked, so the hand is drawn over the table for as long as the player is reading it.
 */
import type { CardFace } from './../../card-face'
import { Card } from './../Card'
import type { Surface } from './surface'

export function Hand({
  faces,
  peek = false,
  raised = false,
  onRaise,
  surface,
}: {
  faces: readonly CardFace[]
  /** Whether the dock currently owns the bottom band (§2, "The action affordance and the hand"). */
  peek?: boolean
  /** Whether the player has asked for the full hand back over the table. */
  raised?: boolean
  onRaise?(raised: boolean): void
  surface: Surface
}) {
  const cards = (
    <ul className="cards cards--hand">
      {faces.map((face) => (
        <li key={face.id}>
          <Card
            face={face}
            // The hand is where a player chooses, so a name here is never abbreviated —
            // `docs/client-design.md` §6. A hand that reads `C…`, `Dis…`, `Sna…` is not a
            // degraded hand, it is an unusable one.
            mayAbbreviate={false}
            state={surface.stateOf(face.id)}
            link={surface.linkOf(face.id)}
            onActivate={surface.activate}
            onInspect={surface.inspect}
            onTrace={surface.trace}
          />
        </li>
      ))}
    </ul>
  )

  return (
    <section className={`hand${peek ? ' hand--peek' : ''}`} aria-label="Your hand">
      {faces.length === 0 ? <p className="hand__empty">Your hand is empty.</p> : cards}

      {/* Offered only while the strip is what is drawn, and it is the same control both ways:
          the gesture that raises the hand is the gesture that puts it back. */}
      {peek && faces.length > 0 && onRaise && (
        <button
          type="button"
          className="hand__raise"
          aria-expanded={raised}
          onClick={() => onRaise(!raised)}
        >
          {raised ? 'Lower' : `Hand (${faces.length})`}
        </button>
      )}
    </section>
  )
}

/**
 * The full hand, over the table, while the strip below it stays where it was.
 *
 * Deliberately not the same element moved and resized: the strip is the region, it keeps its box,
 * and this is a thing drawn on top of the board — the same class of object as an opened pile.
 * That is what keeps the geometry honest while it is open, and what makes closing it cost
 * nothing.
 */
export function RaisedHand({
  faces,
  onLower,
  surface,
}: {
  faces: readonly CardFace[]
  onLower(): void
  surface: Surface
}) {
  return (
    <div className="raised" role="group" aria-label="Your hand, raised">
      <ul className="cards cards--hand">
        {faces.map((face) => (
          <li key={face.id}>
            <Card
              face={face}
              mayAbbreviate={false}
              state={surface.stateOf(face.id)}
              link={surface.linkOf(face.id)}
              onActivate={(id) => {
                onLower()
                surface.activate(id)
              }}
              onInspect={surface.inspect}
              onTrace={surface.trace}
            />
          </li>
        ))}
      </ul>
      <button type="button" className="raised__close" onClick={onLower}>
        Close
      </button>
    </div>
  )
}
