/**
 * The roster of the table you are at: who is in each seat, and what each seat still owes.
 *
 * The status words and the waiting list are the seat flags the server sent, restated
 * (`lobby.ts`). The room starts when the server's own gate closes, not when this list empties —
 * what it shows is why the wait is happening, which is the thing a player sitting at a table with
 * nothing moving actually wants to know.
 *
 * Seating a bot is offered here because an empty seat is where you would look for it, and it is
 * offered *at all* only because the server advertised `add_ai` — the client never works out that
 * it is the host. The kinds are the ones the catalog advertises; the deck is one of the bundled
 * starters, validated by the server exactly like a human's.
 */
import { useState } from 'react'

import type { SeatRow } from './../../lobby'
import type { AiOption } from './../../protocol'
import type { StarterDeck } from './../../decks'

export function SeatRoster({
  rows,
  waiting,
  aiOptions,
  starters,
  canAddAi,
  canRemoveAi,
  onSeatAi,
  onRemoveAi,
}: {
  rows: readonly SeatRow[]
  waiting: readonly string[]
  aiOptions: readonly AiOption[]
  starters: readonly StarterDeck[]
  canAddAi: boolean
  canRemoveAi: boolean
  onSeatAi(seat: number, kind: string, cards: readonly string[]): void
  onRemoveAi(seat: number): void
}) {
  const [kind, setKind] = useState(aiOptions[0]?.id ?? '')
  const [deckId, setDeckId] = useState(starters[0]?.id ?? '')
  const deck = starters.find((candidate) => candidate.id === deckId)
  const seating = canAddAi && aiOptions.length > 0 && deck !== undefined

  return (
    <section aria-labelledby="roster-heading" className="roster">
      <h3 id="roster-heading">Seats</h3>
      <ul className="roster__list">
        {rows.map((row) => (
          <li key={row.seat} className={`seat-row${row.you ? ' seat-row--you' : ''}`}>
            <span className="seat-row__index">Seat {row.seat + 1}</span>
            <span className="seat-row__who">
              {row.label}
              {row.you && ' (you)'}
            </span>
            <span className="seat-row__status">
              {row.status.map((word) => (
                <span key={word} className="badge badge--marker">
                  {word}
                </span>
              ))}
              {row.awaiting && <span className="seat-row__awaiting">{row.awaiting}</span>}
            </span>
            <span className="seat-row__controls">
              {seating && !row.occupied && (
                <button type="button" onClick={() => onSeatAi(row.seat, kind, deck.cards)}>
                  Seat an AI opponent
                </button>
              )}
              {canRemoveAi && row.ai !== undefined && (
                <button type="button" onClick={() => onRemoveAi(row.seat)}>
                  Remove
                </button>
              )}
            </span>
          </li>
        ))}
      </ul>

      {seating && (
        <p className="roster__ai">
          <label>
            Opponent{' '}
            <select value={kind} onChange={(event) => setKind(event.target.value)}>
              {aiOptions.map((option) => (
                <option key={option.id} value={option.id}>
                  {option.name}
                </option>
              ))}
            </select>
          </label>{' '}
          <label>
            playing{' '}
            <select value={deckId} onChange={(event) => setDeckId(event.target.value)}>
              {starters.map((candidate) => (
                <option key={candidate.id} value={candidate.id}>
                  {candidate.name}
                </option>
              ))}
            </select>
          </label>{' '}
          <span className="roster__ai-note">
            {aiOptions.find((option) => option.id === kind)?.description}
          </span>
        </p>
      )}

      {waiting.length > 0 && <p className="roster__waiting">Waiting on — {waiting.join(' · ')}</p>}
    </section>
  )
}
