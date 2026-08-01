/**
 * The full face of one object, opened from anywhere.
 *
 * Every other variant clamps or drops something to fit its surface. This is where the rest of
 * it lives, which is what makes the clamping honest: a truncated rules text or a battlefield
 * tile with no cost is a summary, not the only copy.
 *
 * Opening it is not a game action. It submits nothing, sends nothing, and is available on any
 * object the view already shows — including one the player cannot act on, which is when reading
 * it matters most. Nothing here is remembered across messages; the inspector holds an id and
 * re-reads the face out of the current view (`Game.tsx`), so a refresh or a new frame produces
 * the same screen rather than a stale card.
 */
import { useEffect } from 'react'

import type { CardFace } from './../card-face'
import { Card } from './Card'

export function CardInspector({ face, onClose }: { face: CardFace; onClose(): void }) {
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape') onClose()
    }
    document.addEventListener('keydown', onKey)
    return () => document.removeEventListener('keydown', onKey)
  }, [onClose])

  return (
    // Dismissing by clicking away is the same non-action as the button; the panel stops the
    // click so a click inside it does not close what it is on.
    <div className="inspector-backdrop" onClick={onClose}>
      <div
        role="dialog"
        aria-modal="true"
        aria-label={`${face.name} — card details`}
        className="inspector"
        onClick={(event) => event.stopPropagation()}
      >
        <Card face={face} />
        <button type="button" className="inspector__close" onClick={onClose} autoFocus>
          Close
        </button>
      </div>
    </div>
  )
}
