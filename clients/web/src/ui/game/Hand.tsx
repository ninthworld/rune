/**
 * Your hand, along the bottom edge.
 *
 * The one zone only you can see, so it gets the edge nearest you and a full row to itself. A
 * large hand scrolls sideways rather than wrapping into a second row that would eat the table
 * above it or push the action dock off the screen.
 */
import type { CardFace } from './../../card-face'
import { Card } from './../Card'
import type { Surface } from './surface'

export function Hand({ faces, surface }: { faces: readonly CardFace[]; surface: Surface }) {
  return (
    <section className="hand" aria-label="Your hand">
      {faces.length === 0 ? (
        <p className="hand__empty">Your hand is empty.</p>
      ) : (
        <ul className="cards cards--hand">
          {faces.map((face) => (
            <li key={face.id}>
              <Card
                face={face}
                variant="hand"
                state={surface.stateOf(face)}
                onInspect={surface.inspect}
              />
            </li>
          ))}
        </ul>
      )}
    </section>
  )
}
