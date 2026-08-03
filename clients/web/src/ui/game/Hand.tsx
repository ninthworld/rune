/**
 * Your hand, along the bottom edge.
 *
 * It centres itself while it fits and pans once it does not — the same strip a battlefield row
 * uses, so reaching a card off the end is one gesture wherever it is (§3, §5).
 *
 * The action bar sits above it permanently rather than trading places with it (§2): the game
 * asking a question must not be the reason a player cannot see what they are holding.
 */
import type { CardFace } from './../../card-face'
import { Card } from './../card/Card'
import { useScrollStrip } from './../card/scrollStrip'
import type { Surface } from './surface'

export function Hand({ faces, surface }: { faces: readonly CardFace[]; surface: Surface }) {
  const { ref, edges } = useScrollStrip<HTMLDivElement>()
  return (
    <div className="hand" role="region" aria-label="Your hand">
      <div className={`strip hand-scroll${edges}`} ref={ref}>
        {faces.map((face) => (
          <Card
            key={face.id}
            face={face}
            anchor={face.id}
            state={surface.stateOf(face.id)}
            link={surface.linkOf(face.id)}
            onTrace={surface.trace}
            onActivate={surface.activate}
            onInspect={surface.inspect}
          />
        ))}
      </div>
    </div>
  )
}
