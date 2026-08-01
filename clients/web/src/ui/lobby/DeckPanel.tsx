/**
 * The deck for your seat: what it holds, where it came from, and the one control that sends it.
 *
 * Two paths reach a game and both end here. A starter deck is one click and is the fast path —
 * it is the same file the engine's own agent-vs-agent test plays, so "a deck that works" needs no
 * building. The builder is the other, and it edits the very same draft, which is why picking a
 * starter and then changing six cards is not a different workflow.
 *
 * **Submit is offered whenever the server offers `submit_deck`.** Never gated on the size note
 * beside it, never on whether a commander has been designated. The client's arithmetic is advice;
 * a rejection is the answer, and it is shown here verbatim as the server worded it.
 */
import { deckRules, deckSize, sizeAdvice, type Catalog, type DeckDraft } from './../../deck'
import type { StarterDeck } from './../../decks'
import type { CatalogFormat } from './../../protocol'

export function DeckPanel({
  draft,
  catalog,
  format,
  starters,
  rejection,
  canSubmit,
  builderOpen,
  onStarter,
  onToggleBuilder,
  onSubmit,
}: {
  draft: DeckDraft
  catalog: Catalog
  format?: CatalogFormat
  starters: readonly StarterDeck[]
  /** The server's last word on a submission from this seat, if it refused one. */
  rejection?: string
  canSubmit: boolean
  builderOpen: boolean
  onStarter(id: string): void
  onToggleBuilder(): void
  onSubmit(): void
}) {
  const size = deckSize(draft)
  const advice = sizeAdvice(draft, format)
  const rules = deckRules(format)

  return (
    <section aria-labelledby="deck-heading" className="deck">
      <h3 id="deck-heading">Deck</h3>

      {rejection && (
        <p role="alert" className="notice deck__rejected">
          The server rejected this deck — {rejection}
        </p>
      )}

      <p className="deck__summary">
        {size === 0 ? (
          'No deck yet.'
        ) : (
          <>
            {size} card{size === 1 ? '' : 's'} · {draft.entries.length} different
            {draft.commander !== undefined && (
              <> · commander {catalog.byId.get(draft.commander)?.name ?? draft.commander}</>
            )}
          </>
        )}
        {advice && <span className="deck__advice"> — {advice}</span>}
      </p>

      {rules.length > 0 && <p className="deck__rules">{rules.join(' · ')}</p>}

      <p className="deck__controls">
        <label>
          Starter deck{' '}
          <select
            aria-label="Starter deck"
            defaultValue=""
            onChange={(event) => event.target.value && onStarter(event.target.value)}
          >
            <option value="">choose…</option>
            {starters.map((candidate) => (
              <option key={candidate.id} value={candidate.id}>
                {candidate.name} — {candidate.summary}
              </option>
            ))}
          </select>
        </label>{' '}
        <button type="button" onClick={onToggleBuilder}>
          {builderOpen ? 'Close the deck builder' : 'Build a deck'}
        </button>{' '}
        {canSubmit && (
          <button type="button" onClick={onSubmit}>
            Submit deck ({size} cards)
          </button>
        )}
      </p>
    </section>
  )
}
