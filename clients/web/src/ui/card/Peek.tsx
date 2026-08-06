/**
 * The card, pinned: held open as large as the screen allows, over a glass scrim
 * (`docs/client-design.md` §6.6).
 *
 * **This is where a two-faced card turns over** (§6.7). The board draws the face that is up and
 * says only *that* there is another one; the other side is Tier 3 — on demand — which is the
 * right tier for a fact a player needs before casting a card and never during combat. So the pin
 * carries the turn, as a control rather than a gesture, because everything reachable by pointer
 * has to be reachable by keyboard (§6.5 rule 4).
 *
 * It draws one `Card` and nothing else. Which side is up is local to this overlay and dies with
 * it: nothing about it is state the board reads, and the next pin opens on the face the server
 * says is up.
 */
import { useState } from 'react'

import type { CardFace } from './../../card-face'
import { Card } from './Card'

export function Peek({ face, onClose }: { face: CardFace; onClose(): void }) {
  const [turned, setTurned] = useState(false)
  const other = face.otherFace
  const shown = turned && other ? other : face

  return (
    <div className="peek" onClick={onClose}>
      <div className="peek-card" onClick={(event) => event.stopPropagation()}>
        <Card face={shown} />
        {/* Named for what it does rather than for what is on the other side: the card itself
            says that, once it is turned. */}
        {other && (
          <button className="peek-turn" onClick={() => setTurned((over) => !over)}>
            Turn over
          </button>
        )}
      </div>
    </div>
  )
}
