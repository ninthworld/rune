/**
 * The deck for your seat: what it holds, where it came from, and the one control that sends it.
 *
 * Two paths reach a game and both end here. A starter deck is one click and is the fast path —
 * it is the same file the engine's own agent-vs-agent test plays, so "a deck that works" needs no
 * building. The builder is the other, and it is a destination in the shell now rather than a
 * panel that unfolds under this one: it edits the very same draft, which is why picking a starter
 * and then changing six cards is not a different workflow.
 *
 * **Submit is offered whenever the server offers `submit_deck`.** Never gated on the size note
 * beside it, never on whether a commander has been designated. The client's arithmetic is advice;
 * a rejection is the answer, and it is shown here verbatim as the server worded it.
 */
import { useState } from 'react'

import { deckRules, deckSize, type Catalog, type DeckDraft } from './../../deck'
import type { StarterDeck } from './../../decks'
import type { CatalogFormat } from './../../protocol'
import { Picker } from './../controls'

export function DeckPanel({
  draft,
  catalog,
  format,
  starters,
  rejection,
  canSubmit,
  onStarter,
  onBuild,
  onSubmit,
}: {
  draft: DeckDraft
  catalog: Catalog
  format?: CatalogFormat
  starters: readonly StarterDeck[]
  /** The server's last word on a submission from this seat, if it refused one. */
  rejection?: string
  canSubmit: boolean
  onStarter(id: string): void
  onBuild(): void
  onSubmit(): void
}) {
  // Which starter was taken, so the control reads as a choice that was made rather than as a
  // button that fired. It says nothing about the deck since: the draft is what the deck is.
  const [taken, setTaken] = useState('')
  const size = deckSize(draft)
  const rules = deckRules(format)

  return (
    <section aria-label="Deck" className="deck">
      <h2>Your deck</h2>

      {rejection && (
        <p role="alert" className="notice deck__rejected">
          The server rejected this deck — {rejection}
        </p>
      )}

      {/* The deck as the two numbers it is, at a size that is read at a glance rather than
          parsed out of a sentence. */}
      <p className="deck__count">
        <span className="deck__cards">{size}</span>
        <span className="deck__unit">cards</span>
        {size > 0 && <span className="deck__different">{draft.entries.length} different</span>}
        {draft.commander !== undefined && (
          <span className="deck__commander">
            {catalog.byId.get(draft.commander)?.name ?? draft.commander}
          </span>
        )}
      </p>

      {rules.length > 0 && (
        <p className="deck__rules">
          {rules.map((rule) => (
            <span key={rule} className="chip">
              {rule}
            </span>
          ))}
        </p>
      )}

      {/* Seven starter decks with a sentence each is seven sentences on a screen whose subject
          is one deck. They open on demand instead, which is where §9.2 rule 1 puts a
          description: on the option, at the moment somebody is choosing between options. */}
      <div className="deck__starters">
        <span className="field__label">Starter deck</span>
        <Picker
          label="Starter deck"
          value={taken}
          placeholder="choose one"
          options={starters.map((candidate) => ({
            value: candidate.id,
            label: candidate.name,
            detail: candidate.summary,
          }))}
          onChange={(id) => {
            setTaken(id)
            onStarter(id)
          }}
        />
      </div>

      <p className="deck__controls">
        {canSubmit && (
          <button type="button" className="page__lead" onClick={onSubmit}>
            Submit deck
          </button>
        )}
        <button type="button" onClick={onBuild}>
          Build a deck
        </button>
      </p>
    </section>
  )
}
