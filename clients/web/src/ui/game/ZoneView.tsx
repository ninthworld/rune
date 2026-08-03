/**
 * Looking inside a zone (`docs/client-design.md` §6.6).
 *
 * One surface serves every case the game has — reading a graveyard or an exile, checking a
 * command zone, looking through a pile the game asked you to choose from — because they differ
 * only in what they are called and whether the game wants a card back. **When the game asked the
 * question, the dialog carries the answer**: the cards become the controls that answer it, and a
 * click on one goes through the same `activate` a click on the board does. When the game did not
 * ask, it is a place to look and nothing is selectable.
 *
 * The grid fills by card width rather than by a fixed column count, so the same panel is four
 * across on a desktop and two on a phone without a second layout to keep in step — **and it may
 * scroll**, because a pile is not the board (§3).
 */
import { useEffect } from 'react'

import type { CardFace } from './../../card-face'
import { Card } from './../card/Card'
import type { Surface } from './surface'

export interface OpenZone {
  /** Whose pile, and which — the two things a head has to say. */
  title: string
  faces: readonly CardFace[]
}

export function ZoneView({
  zone,
  asking,
  onClose,
  surface,
}: {
  zone: OpenZone
  /** Whether anything in here currently answers a question the server asked. */
  asking: boolean
  onClose(): void
  surface: Surface
}) {
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape') onClose()
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [onClose])

  return (
    <div className="zone-view" onClick={onClose}>
      <div
        className={`zone-panel${asking ? ' zone-choosing' : ''}`}
        role="dialog"
        aria-label={zone.title}
        onClick={(event) => event.stopPropagation()}
      >
        <div className="zone-head">
          <span className="zone-title">{zone.title}</span>
          <span className="zone-tally">
            {zone.faces.length} {zone.faces.length === 1 ? 'card' : 'cards'}
          </span>
          <button className="zone-close" onClick={onClose} title="Close">
            ✕
          </button>
        </div>

        <div className="zone-body">
          {zone.faces.length === 0 && <p className="zone-empty">Nothing here.</p>}
          {zone.faces.map((face) => (
            <span key={face.id} className="zone-slot">
              <Card
                face={face}
                state={surface.stateOf(face.id)}
                link={surface.linkOf(face.id)}
                onTrace={surface.trace}
                onActivate={surface.activate}
                onInspect={surface.inspect}
              />
            </span>
          ))}
        </div>
      </div>
    </div>
  )
}
