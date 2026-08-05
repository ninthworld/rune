/**
 * The two choices made at a table, each as a dialog off the seat it belongs to
 * (`docs/client-design.md` §9.7): which deck is yours, and who fills an empty seat.
 *
 * They are one file because they are one shape — a list of rows in the glass of §5.5, a footer
 * saying what is picked, and one raised button that commits it. Neither is a native control:
 * a `<select>` clips its own arrow at 120% zoom, and what an option *is* rides on the option
 * rather than beside the control forever.
 *
 * Everything in both lists comes from data this client was handed rather than from anything it
 * decided: the decks are the bundled starters (`decks.ts`, the same file the engine's
 * agent-vs-agent test plays), and the AI kinds and their descriptions are the catalog's own
 * words. A deck's legality is the server's `LobbyRejection` and is never pre-judged here.
 */
import { useState } from 'react'

import type { StarterDeck } from './../../decks'
import type { AiOption } from './../../protocol'

function Panel({
  title,
  tally,
  hint,
  action,
  disabled,
  onClose,
  onCommit,
  children,
}: {
  title: string
  tally: string
  hint: string
  action: string
  disabled?: boolean
  onClose(): void
  onCommit(): void
  children: React.ReactNode
}) {
  return (
    <div className="zone-view" onClick={onClose}>
      <div
        className="zone-panel deck-panel"
        role="dialog"
        aria-label={title}
        onClick={(event) => event.stopPropagation()}
      >
        <div className="zone-head">
          <span className="zone-title">{title}</span>
          <span className="zone-tally">{tally}</span>
          <button className="zone-close" onClick={onClose} title="Close">
            ✕
          </button>
        </div>
        <div className="deck-list">{children}</div>
        <div className="zone-foot">
          <span className="zone-hint">{hint}</span>
          <div className="zone-acts">
            <button className="action-done action-alt" onClick={onClose}>
              Cancel
            </button>
            <button className="action-done" disabled={disabled} onClick={onCommit}>
              {action}
            </button>
          </div>
        </div>
      </div>
    </div>
  )
}

/** Choosing which deck is yours. Picking one submits it — the server is what says it is legal. */
export function DeckPicker({
  decks,
  current,
  onClose,
  onPick,
}: {
  decks: readonly StarterDeck[]
  current?: string
  onClose(): void
  onPick(deck: StarterDeck): void
}) {
  const [pick, setPick] = useState(() => decks.find((deck) => deck.id === current) ?? decks[0])
  return (
    <Panel
      title="Choose a deck"
      tally={`${decks.length} ${decks.length === 1 ? 'deck' : 'decks'}`}
      hint={pick?.summary ?? 'No deck to choose from yet.'}
      action="Choose"
      disabled={pick === undefined}
      onClose={onClose}
      onCommit={() => {
        if (pick) onPick(pick)
        onClose()
      }}
    >
      {decks.map((deck) => (
        <button
          key={deck.id}
          className={`deck-row${deck.id === pick?.id ? ' deck-on' : ''}`}
          onClick={() => setPick(deck)}
        >
          <span className="deck-name">{deck.name}</span>
          <span className="deck-count">{deck.cards.length} cards</span>
        </button>
      ))}
    </Panel>
  )
}

/**
 * Filling an empty seat.
 *
 * Two lists rather than two dialogs: who is playing, and what they are playing. The kinds are
 * offered at all only because the server advertised `add_ai` — this client never works out that
 * it is the host.
 */
export function AiPicker({
  kinds,
  decks,
  onClose,
  onSeat,
}: {
  kinds: readonly AiOption[]
  decks: readonly StarterDeck[]
  onClose(): void
  /**
   * The AI's deck as the wire wants it: the flat list, and the commander the deck names.
   * A deck that names one is seated with it — dropping the designation here would seat a
   * commander deck as an ordinary one and leave the table wondering where its commander went.
   */
  onSeat(kind: string, cards: readonly string[], commander?: string): void
}) {
  const [kind, setKind] = useState(kinds[0]?.id)
  const [deckId, setDeckId] = useState(decks[0]?.id)
  const deck = decks.find((candidate) => candidate.id === deckId)
  const chosen = kinds.find((candidate) => candidate.id === kind)

  return (
    <Panel
      title="Seat an AI opponent"
      tally={`${kinds.length} to choose from`}
      hint={chosen?.description ?? ''}
      action="Seat"
      disabled={kind === undefined || deck === undefined}
      onClose={onClose}
      onCommit={() => {
        if (kind !== undefined && deck) onSeat(kind, deck.cards, deck.commander)
        onClose()
      }}
    >
      <span className="deck-group">Opponent</span>
      {kinds.map((option) => (
        <button
          key={option.id}
          className={`deck-row${option.id === kind ? ' deck-on' : ''}`}
          onClick={() => setKind(option.id)}
        >
          <span className="deck-name">{option.name}</span>
          <span className="deck-count">opponent</span>
        </button>
      ))}
      <span className="deck-group">Deck</span>
      {decks.map((candidate) => (
        <button
          key={candidate.id}
          className={`deck-row${candidate.id === deckId ? ' deck-on' : ''}`}
          onClick={() => setDeckId(candidate.id)}
        >
          <span className="deck-name">{candidate.name}</span>
          <span className="deck-count">{candidate.cards.length} cards</span>
        </button>
      ))}
    </Panel>
  )
}
