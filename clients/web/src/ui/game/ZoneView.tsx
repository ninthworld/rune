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
 * **An ordering is answered here too, by clicking in order** (§6.7). Each card takes an ordinal
 * as it is clicked and clicking a badged card again takes it back out, which renumbers the rest
 * for free — the badge is the position in the answer being assembled and nothing else. There is
 * no drag: it is a second gesture for a job one click already does, it is the gesture that works
 * worst on a phone, and the keyboard equivalent §6.5 rule 4 would then owe *is* the click. The
 * footer says which end is which in words, because a badge saying `1` does not say what `1`
 * means, and the commit stays explicit.
 *
 * The grid fills by card width rather than by a fixed column count, so the same panel is four
 * across on a desktop and two on a phone without a second layout to keep in step — **and it may
 * scroll**, because a pile is not the board (§3).
 */
import { useEffect } from 'react'

import type { CardFace } from './../../card-face'
import { Card } from './../card/Card'
import type { Surface } from './surface'

/** What the game is asking about this pile, when it is asking something. */
export interface ZoneQuestion {
  /**
   * What answering does, in words. For an ordering it is the sentence that orients the badges —
   * *the first you pick goes deepest* — which is the only wording this surface needs and which
   * replaces the heading a player would not read twice (§6.7).
   */
  note: string
  /** What has been answered against what the server asked for. */
  tally: string
  /** Where each card sits in the order so far, by id. Empty for a question that is not one. */
  ordinals: ReadonlyMap<string, number>
  /** Whether the answer is one the server said it would take. */
  ready: boolean
  commit(): void
}

export interface OpenZone {
  /** Whose pile, and which — the two things a head has to say. */
  title: string
  faces: readonly CardFace[]
  question?: ZoneQuestion
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

  const question = zone.question

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
          {zone.faces.map((face) => {
            const ordinal = question?.ordinals.get(face.id)
            return (
              <span key={face.id} className="zone-slot">
                <Card
                  face={face}
                  state={surface.stateOf(face.id)}
                  link={surface.linkOf(face.id)}
                  onTrace={surface.trace}
                  onActivate={surface.activate}
                  onInspect={surface.inspect}
                  {...(ordinal === undefined ? {} : { note: `position ${ordinal} in the order` })}
                />
                {/* The position this card holds in the answer, drawn on the card it belongs to.
                    The card's own accessible name carries it too, so the badge is never the only
                    copy of the fact. */}
                {ordinal !== undefined && (
                  <span className="zone-ord" aria-hidden="true">
                    {ordinal}
                  </span>
                )}
              </span>
            )
          })}
        </div>

        {question && (
          <div className="zone-foot">
            <span className="zone-note" role="status">
              {question.note} <b>{question.tally}</b>
            </span>
            <button className="action-done" disabled={!question.ready} onClick={question.commit}>
              Confirm
            </button>
          </div>
        )}
      </div>
    </div>
  )
}
