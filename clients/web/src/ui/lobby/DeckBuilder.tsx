/**
 * The deck builder: the server's card pool on the left, the deck being built on the right.
 *
 * Both columns draw the same `Card` the game table draws, from the same `CardFace` model — a card
 * you are choosing and a card you will later play are the same card, and a builder with its own
 * idea of what one looks like is a second presentation to keep in step.
 *
 * Nothing here is legality. The search matches strings the catalog sent; the rules strip repeats
 * the numbers the format published; the copy control adds a copy whatever the limit says, because
 * whether *this* card is exempt from it is a rules question the server answers. Submission lives
 * outside this panel, so the one verdict on a deck is the one the server sends back.
 *
 * The result list is capped and says so. The M19 pool is a few hundred cards, a full card frame
 * each; a browser that silently rendered the first sixty would read as a pool that small.
 */
import { useState } from 'react'

import { catalogFace } from './../../card-face'
import {
  copiesOf,
  deckRules,
  deckSize,
  findCards,
  sizeAdvice,
  type Catalog,
  type DeckDraft,
} from './../../deck'
import type { CatalogFormat } from './../../protocol'
import { Card } from './../Card'

/** How many faces the pool draws at once before asking for a narrower search. */
const SHOWN = 60

export function DeckBuilder({
  catalog,
  format,
  draft,
  onAdd,
  onRemove,
  onCommander,
  onInspect,
  onClose,
}: {
  catalog: Catalog
  /** The room's format, when the catalog described it. Its rules are quoted, never applied. */
  format?: CatalogFormat
  draft: DeckDraft
  onAdd(identity: string): void
  onRemove(identity: string): void
  onCommander(identity: string | undefined): void
  onInspect(identity: string): void
  onClose(): void
}) {
  const [text, setText] = useState('')
  const [keyword, setKeyword] = useState('')

  const matches = findCards(catalog, { text, keyword: keyword || undefined })
  const shown = matches.slice(0, SHOWN)
  const advice = sizeAdvice(draft, format)

  return (
    <section className="builder" aria-label="Deck builder">
      <div className="builder__head">
        <h3>Deck builder</h3>
        <button type="button" className="builder__close" onClick={onClose}>
          Done
        </button>
      </div>

      <div className="builder__body">
        <div className="builder__pool">
          <p className="builder__search">
            <label>
              Search{' '}
              <input
                aria-label="Search cards"
                placeholder="name, type, or rules text"
                value={text}
                onChange={(event) => setText(event.target.value)}
              />
            </label>{' '}
            <label>
              Keyword{' '}
              <select
                aria-label="Keyword"
                value={keyword}
                onChange={(event) => setKeyword(event.target.value)}
              >
                <option value="">any</option>
                {catalog.keywords.map((word) => (
                  <option key={word} value={word}>
                    {word}
                  </option>
                ))}
              </select>
            </label>
          </p>

          <p className="builder__count" role="status">
            {matches.length === 0
              ? 'No card in the catalog matches.'
              : matches.length > shown.length
                ? `Showing ${shown.length} of ${matches.length} cards — narrow the search to see the rest.`
                : `${matches.length} card${matches.length === 1 ? '' : 's'}.`}
          </p>

          <ul className="cards builder__results">
            {shown.map((card) => (
              <li key={card.functional_id}>
                <Card
                  face={catalogFace(card)}
                  variant="hand"
                  onActivate={() => onInspect(card.functional_id)}
                />
                <button
                  type="button"
                  className="builder__add"
                  onClick={() => onAdd(card.functional_id)}
                >
                  Add
                  {copiesOf(draft, card.functional_id) > 0 &&
                    ` (${copiesOf(draft, card.functional_id)})`}
                </button>
              </li>
            ))}
          </ul>
        </div>

        <div className="builder__deck">
          <h4>
            Deck — {deckSize(draft)} card{deckSize(draft) === 1 ? '' : 's'}
          </h4>
          {advice && <p className="builder__advice">{advice}</p>}
          {deckRules(format).length > 0 && (
            <p className="builder__rules">{deckRules(format).join(' · ')}</p>
          )}

          {draft.entries.length === 0 ? (
            <p className="builder__empty">Nothing in the deck yet.</p>
          ) : (
            <ul className="builder__entries">
              {draft.entries.map((entry) => (
                <li key={entry.identity} className="deck-entry">
                  <span className="deck-entry__count">{entry.count}×</span>
                  <button
                    type="button"
                    className="deck-entry__name"
                    onClick={() => onInspect(entry.identity)}
                  >
                    {catalog.byId.get(entry.identity)?.name ?? entry.identity}
                  </button>
                  <span className="deck-entry__controls">
                    <button
                      type="button"
                      aria-label={`Remove a copy of ${catalog.byId.get(entry.identity)?.name ?? entry.identity}`}
                      onClick={() => onRemove(entry.identity)}
                    >
                      −
                    </button>
                    <button
                      type="button"
                      aria-label={`Add a copy of ${catalog.byId.get(entry.identity)?.name ?? entry.identity}`}
                      onClick={() => onAdd(entry.identity)}
                    >
                      +
                    </button>
                    {format?.requires_commander && (
                      <button
                        type="button"
                        aria-pressed={draft.commander === entry.identity}
                        onClick={() =>
                          onCommander(
                            draft.commander === entry.identity ? undefined : entry.identity,
                          )
                        }
                      >
                        Commander
                      </button>
                    )}
                  </span>
                </li>
              ))}
            </ul>
          )}
        </div>
      </div>
    </section>
  )
}
