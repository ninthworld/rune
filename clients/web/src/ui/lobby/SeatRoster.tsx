/**
 * The seats of the table you are at, and what each of them still owes.
 *
 * **Waiting-on is drawn on the seat it belongs to** (`docs/client-design.md` §9.5). The old
 * screen printed *"Waiting on — Seat 1 — No deck yet · Seat 2 — Nobody here yet"* underneath a
 * list of seats that already carried those facts, which is §9.2's second rule exactly: a fact
 * that needs a sentence to be legible means the thing drawing it is wrong, and the sentence is
 * not the fix. So a seat carries two marks, lit or unlit, and an empty seat carries none —
 * because what an empty seat is waiting for is somebody, which is the seat itself.
 *
 * The marks are the flags the server stated, restated (`lobby.ts`). The room starts when the
 * server's own gate closes, never when this list looks complete.
 *
 * Seating a bot is offered on the empty seat, because that is where a player looks for it, and
 * it is offered *at all* only because the server advertised `add_ai` — the client never works
 * out that it is the host. Choosing the opponent happens **when the choice is being made**: the
 * kinds and what each one plays like are the catalog's own words, on the options, at the moment
 * a player is picking between them, rather than printed beside a control forever.
 */
import { useState } from 'react'

import type { SeatRow } from './../../lobby'
import type { AiOption } from './../../protocol'
import type { StarterDeck } from './../../decks'
import { Choice } from './../controls'

export function SeatRoster({
  rows,
  aiOptions,
  starters,
  canAddAi,
  canRemoveAi,
  onSeatAi,
  onRemoveAi,
}: {
  rows: readonly SeatRow[]
  aiOptions: readonly AiOption[]
  starters: readonly StarterDeck[]
  canAddAi: boolean
  canRemoveAi: boolean
  onSeatAi(seat: number, kind: string, cards: readonly string[]): void
  onRemoveAi(seat: number): void
}) {
  const [filling, setFilling] = useState<number>()
  const [kind, setKind] = useState(aiOptions[0]?.id ?? '')
  const [deckId, setDeckId] = useState(starters[0]?.id ?? '')
  const deck = starters.find((candidate) => candidate.id === deckId)
  const seating = canAddAi && aiOptions.length > 0 && deck !== undefined

  return (
    <section aria-label="Seats" className="seats">
      <ul className="seats__list">
        {rows.map((row) => (
          <li
            key={row.seat}
            className={`seat${row.you ? ' seat--you' : ''}${row.occupied ? '' : ' seat--empty'}`}
          >
            <span className="seat__index" aria-hidden="true">
              {row.seat + 1}
            </span>
            <span className="seat__who">
              <span className="visually-hidden">Seat {row.seat + 1} — </span>
              {row.occupied ? row.label : 'Empty'}
              {row.you && <span className="seat__you"> you</span>}
            </span>
            {row.ai !== undefined && <span className="seat__kind">AI</span>}

            <span className="seat__marks">
              {row.marks.map((mark) => (
                <span key={mark.label} className={`mark${mark.met ? ' mark--met' : ''}`}>
                  <span aria-hidden="true">{mark.label}</span>
                  {/* The whole fact, for assistive technology: lit and unlit are not something
                      a screen reader can perceive, and the word alone is ambiguous without it. */}
                  <span className="visually-hidden">{mark.detail}</span>
                </span>
              ))}
            </span>

            <span className="seat__controls">
              {seating && !row.occupied && filling !== row.seat && (
                <button type="button" onClick={() => setFilling(row.seat)}>
                  Seat an AI opponent
                </button>
              )}
              {canRemoveAi && row.ai !== undefined && (
                <button type="button" onClick={() => onRemoveAi(row.seat)}>
                  Remove
                </button>
              )}
            </span>

            {seating && filling === row.seat && (
              <div className="seat__fill">
                <Choice
                  label="Opponent"
                  columns
                  value={kind}
                  options={aiOptions.map((option) => ({
                    value: option.id,
                    label: option.name,
                    ...(option.description ? { detail: option.description } : {}),
                  }))}
                  onChange={setKind}
                />
                <Choice
                  label="Deck"
                  columns
                  value={deckId}
                  options={starters.map((candidate) => ({
                    value: candidate.id,
                    label: candidate.name,
                    detail: candidate.summary,
                  }))}
                  onChange={setDeckId}
                />
                <p className="seat__fill-go">
                  <button
                    type="button"
                    className="page__lead"
                    autoFocus
                    onClick={() => {
                      onSeatAi(row.seat, kind, deck.cards)
                      setFilling(undefined)
                    }}
                  >
                    Seat
                  </button>
                  <button type="button" onClick={() => setFilling(undefined)}>
                    Cancel
                  </button>
                </p>
              </div>
            )}
          </li>
        ))}
      </ul>
    </section>
  )
}
