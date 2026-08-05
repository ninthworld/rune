/**
 * Where a deck comes from and where it goes: this device, or a file.
 *
 * Two dialogs over the builder. **Loading** offers the decks this browser kept, the bundled
 * starters, and a file off disk; **saving** writes to this browser or downloads a `.dck`.
 *
 * Deleting a kept deck asks first, and asks in place rather than in a second dialog over the
 * first: there is no undo, and the row that is about to go is the thing worth looking at while
 * answering.
 */
import { useRef, useState } from 'react'

import { STARTER_DECKS, type StarterDeck } from './../../decks'
import { savedSize, type SavedDeck } from './../../deck-store'

export function LoadDeck({
  saved,
  onSaved,
  onStarter,
  onFile,
  onDelete,
  onClose,
}: {
  saved: readonly SavedDeck[]
  onSaved(deck: SavedDeck): void
  onStarter(deck: StarterDeck): void
  onFile(name: string, text: string): void
  onDelete(name: string): void
  onClose(): void
}) {
  const [asking, setAsking] = useState<string | undefined>(undefined)
  const picker = useRef<HTMLInputElement>(null)

  return (
    <div className="zone-view" onClick={onClose}>
      <div
        className="zone-panel deck-files"
        role="dialog"
        aria-label="Load a deck"
        onClick={(event) => event.stopPropagation()}
      >
        <div className="zone-head">
          <span className="zone-title">Load a deck</span>
          <button className="zone-close" onClick={onClose} title="Close">
            ✕
          </button>
        </div>

        <div className="files-body">
          <div className="files-group">
            <span className="files-label">On this device</span>
            {saved.map((deck) => (
              <div className="files-row" key={deck.name}>
                <span className="files-name">{deck.name}</span>
                <span className="files-note">{savedSize(deck)} cards</span>
                {asking === deck.name ? (
                  <>
                    <span className="files-ask">Delete for good?</span>
                    <button className="view-btn" onClick={() => setAsking(undefined)}>
                      Keep
                    </button>
                    <button
                      className="view-btn files-danger"
                      onClick={() => {
                        onDelete(deck.name)
                        setAsking(undefined)
                      }}
                    >
                      Delete
                    </button>
                  </>
                ) : (
                  <>
                    <button className="view-btn" onClick={() => setAsking(deck.name)}>
                      Delete
                    </button>
                    <button className="action-done" onClick={() => onSaved(deck)}>
                      Load
                    </button>
                  </>
                )}
              </div>
            ))}
            {saved.length === 0 && (
              <div className="files-empty">This browser is keeping no decks yet.</div>
            )}
          </div>

          <div className="files-group">
            <span className="files-label">Starter decks</span>
            {STARTER_DECKS.map((deck) => (
              <div className="files-row" key={deck.id}>
                <span className="files-name">{deck.name}</span>
                <span className="files-note">{deck.summary}</span>
                <button className="action-done" onClick={() => onStarter(deck)}>
                  Load
                </button>
              </div>
            ))}
          </div>
        </div>

        <div className="zone-foot">
          <span className="zone-hint">A file is read here and never sent anywhere.</span>
          <div className="zone-acts">
            <input
              ref={picker}
              className="files-picker"
              type="file"
              accept=".dck,.txt,text/plain"
              aria-label="Deck file"
              onChange={(event) => {
                const file = event.target.files?.[0]
                if (!file) return
                void file.text().then((text) => onFile(file.name.replace(/\.[^.]+$/, ''), text))
              }}
            />
            <button className="action-done action-alt" onClick={() => picker.current?.click()}>
              From a file…
            </button>
          </div>
        </div>
      </div>
    </div>
  )
}

export function SaveDeck({
  name,
  onName,
  onDevice,
  onFile,
  onClose,
}: {
  name: string
  onName(name: string): void
  onDevice(): void
  onFile(): void
  onClose(): void
}) {
  const named = name.trim() !== ''

  return (
    <div className="zone-view" onClick={onClose}>
      <div
        className="zone-panel deck-files"
        role="dialog"
        aria-label="Save the deck"
        onClick={(event) => event.stopPropagation()}
      >
        <div className="zone-head">
          <span className="zone-title">Save the deck</span>
          <button className="zone-close" onClick={onClose} title="Close">
            ✕
          </button>
        </div>

        <div className="files-body">
          <label className="files-field">
            <span className="files-label">Called</span>
            <input
              className="connect-input"
              aria-label="Deck name"
              value={name}
              placeholder="Untitled deck"
              onChange={(event) => onName(event.target.value)}
            />
          </label>
        </div>

        <div className="zone-foot">
          <span className="zone-hint">
            {named ? 'A name already kept here is replaced.' : 'Name the deck to save it.'}
          </span>
          <div className="zone-acts">
            <button className="action-done action-alt" disabled={!named} onClick={onFile}>
              Download .dck
            </button>
            <button className="action-done" disabled={!named} onClick={onDevice}>
              Keep on this device
            </button>
          </div>
        </div>
      </div>
    </div>
  )
}
