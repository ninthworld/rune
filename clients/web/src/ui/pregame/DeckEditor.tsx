/**
 * Adjusting the deck, in a dialog off your own seat (`docs/client-design.md` §9.7).
 *
 * Two lists and a preview: what is in the deck, what the catalog has, one click moving a copy
 * across, and the card under the pointer drawn whole beside them. On a phone the preview is what
 * gives way; the lists are the tool.
 *
 * §9.7 describes the prototype's two lists as main deck and sideboard. **There is no sideboard
 * on the wire** — `submit_deck` carries one flat list of identities — so the second list is the
 * catalog the deck is built from, which is the same gesture over the pair of lists that really
 * exist. The builder itself still awaits its own design pass; what is settled is where the
 * ordinary adjustment lives.
 *
 * **It computes no legality.** The count is arithmetic on a count, the copy limit the format
 * published is quoted and never enforced, and the commander picker offers the deck's own cards —
 * the verdict is the server's `LobbyRejection`, raised on the screen this was opened from.
 */
import { useMemo, useState } from 'react'

import { catalogFace } from './../../card-face'
import {
  copiesOf,
  deckSize,
  findCards,
  sizeAdvice,
  withCard,
  withCommander,
  withoutCard,
  type Catalog,
  type DeckDraft,
} from './../../deck'
import { costTint, manaSymbols } from './../../mana'
import type { CatalogCard, CatalogFormat } from './../../protocol'
import { Card } from './../card/Card'
import { Pip } from './../card/Pips'

/** The colours of the pips a card prints, in printed order — the same shallow reading as §6. */
function Colors({ card }: { card: CatalogCard }) {
  const colors = [...new Set(manaSymbols(card.mana_cost).flatMap((symbol) => [...symbol.colors]))]
  return (
    <span className="deck-colors">
      {colors.map((color) => (
        <Pip key={color} symbol={color.toUpperCase()} />
      ))}
    </span>
  )
}

export function DeckEditor({
  draft,
  catalog,
  format,
  name,
  onChange,
  onSave,
  onClose,
}: {
  draft: DeckDraft
  catalog: Catalog
  format?: CatalogFormat
  name: string
  onChange(draft: DeckDraft): void
  onSave(): void
  onClose(): void
}) {
  const [query, setQuery] = useState('')
  const [shown, setShown] = useState<string | undefined>(draft.entries[0]?.identity)

  const pool = useMemo(
    () => findCards(catalog, { ...(query.trim() === '' ? {} : { text: query.trim() }) }),
    [catalog, query],
  )
  const size = deckSize(draft)
  const advice = sizeAdvice(draft, format)
  const looking = shown === undefined ? undefined : catalog.byId.get(shown)

  return (
    <div className="zone-view" onClick={onClose}>
      <div
        className="zone-panel edit-panel"
        role="dialog"
        aria-label="Deck"
        onClick={(event) => event.stopPropagation()}
      >
        <div className="zone-head">
          <span className="zone-title">{name}</span>
          <span className="zone-tally">click a card to move it across</span>
          <button className="zone-close" onClick={onClose} title="Close">
            ✕
          </button>
        </div>

        <div className="edit-body">
          <div className="edit-pane">
            <div className="edit-head">
              <span className="edit-title">In the deck</span>
              <span className="edit-count">{size}</span>
            </div>
            <div className="edit-list">
              {draft.entries.map((entry) => {
                const card = catalog.byId.get(entry.identity)
                const isCommander = draft.commander === entry.identity
                return (
                  <button
                    key={entry.identity}
                    className="edit-row"
                    title="Take one copy out"
                    onMouseEnter={() => setShown(entry.identity)}
                    onClick={() => onChange(withoutCard(draft, entry.identity))}
                  >
                    <span className="edit-n">{entry.count}</span>
                    {card ? <Colors card={card} /> : <span className="deck-colors" />}
                    <span className="edit-name">{card?.name ?? entry.identity}</span>
                    {format?.requires_commander && (
                      <span
                        role="button"
                        tabIndex={0}
                        className={`edit-cmdr${isCommander ? ' edit-cmdr-on' : ''}`}
                        title={isCommander ? 'The commander' : 'Make this the commander'}
                        onClick={(event) => {
                          event.stopPropagation()
                          onChange(withCommander(draft, isCommander ? undefined : entry.identity))
                        }}
                        onKeyDown={(event) => {
                          if (event.key !== 'Enter' && event.key !== ' ') return
                          event.preventDefault()
                          event.stopPropagation()
                          onChange(withCommander(draft, isCommander ? undefined : entry.identity))
                        }}
                      >
                        ★
                      </span>
                    )}
                    <span className="edit-arrow">→</span>
                  </button>
                )
              })}
              {draft.entries.length === 0 && <div className="edit-empty">Nothing here yet.</div>}
            </div>
          </div>

          <div className="edit-pane">
            <div className="edit-head">
              <span className="edit-title">Cards</span>
              <input
                className="connect-input edit-search"
                aria-label="Search cards"
                value={query}
                onChange={(event) => setQuery(event.target.value)}
                placeholder="Search"
              />
              <span className="edit-count">{pool.length}</span>
            </div>
            <div className="edit-list">
              {pool.map((card) => (
                <button
                  key={card.functional_id}
                  className="edit-row"
                  title="Put one copy in"
                  onMouseEnter={() => setShown(card.functional_id)}
                  onClick={() => onChange(withCard(draft, card.functional_id))}
                >
                  <span className="edit-n">{copiesOf(draft, card.functional_id) || ''}</span>
                  <Colors card={card} />
                  <span className="edit-name">{card.name}</span>
                  <span className="edit-arrow">←</span>
                </button>
              ))}
              {pool.length === 0 && <div className="edit-empty">No card matches that.</div>}
            </div>
          </div>

          <div className="edit-pane edit-side">
            <div className="edit-head">
              <span className="edit-title">{looking?.name ?? 'Card'}</span>
            </div>
            <div className={`edit-preview card-${costTint(looking?.mana_cost)}`}>
              {looking && <Card face={catalogFace(looking)} />}
            </div>
          </div>
        </div>

        <div className="zone-foot">
          <span className="zone-hint">
            {advice ?? `${size} ${size === 1 ? 'card' : 'cards'} in the deck`}
          </span>
          <div className="zone-acts">
            <button className="action-done action-alt" onClick={onClose}>
              Close
            </button>
            <button
              className="action-done"
              onClick={() => {
                onSave()
                onClose()
              }}
            >
              Submit deck
            </button>
          </div>
        </div>
      </div>
    </div>
  )
}
