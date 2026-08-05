/**
 * Adjusting the deck at the table, in a dialog off your own seat (`docs/client-design.md` §9.7).
 *
 * **This is the small edit, not the build.** Between games the change a player makes is moving
 * cards across the sideboard line, so that is what this is: the deck on the left, the cards
 * beside it on the right, one click sending a copy the other way, and the card under the pointer
 * drawn whole between them. Anything larger — searching a catalog of hundreds, rebuilding a curve
 * — is the deck editor, one button away, and coming back submits what you built.
 *
 * **There is no sideboard on the wire.** `submit_deck` carries one flat list and a commander, so
 * moving a card out of the deck here changes what is submitted and moving one in changes it back;
 * the cards beside the deck are this device's own note. That is stated plainly in the footer
 * rather than left for a player to discover when their sideboard does not arrive.
 *
 * **It computes no legality.** The counts are arithmetic on counts, the copy limit the format
 * published is quoted and never enforced, and the verdict is the server's `LobbyRejection`,
 * raised on the screen this was opened from.
 */
import { useState, type ReactNode } from 'react'

import { catalogFace } from './../../card-face'
import {
  deckSize,
  moved,
  sideEntries,
  sizeAdvice,
  withCommander,
  type Catalog,
  type DeckDraft,
  type DeckEntry,
  type DeckList,
} from './../../deck'
import { costTint } from './../../mana'
import type { CatalogFormat } from './../../protocol'
import { Card } from './../card/Card'
import { CardTitle } from './../card/TitleBand'

/** The two things that are not the deck, sharing the pane beside it. */
const PANES = [
  { id: 'side', label: 'Sideboard' },
  { id: 'commander', label: 'Commander' },
] as const

function List({
  title,
  header,
  count,
  entries,
  catalog,
  arrow,
  format,
  commander,
  empty,
  onLook,
  onMove,
  onCommander,
}: {
  title: string
  /** Drawn in place of the title where the pane is more than one thing. */
  header?: ReactNode
  count: number
  entries: readonly DeckEntry[]
  catalog: Catalog
  /** Which way a click sends a copy, drawn on every row. */
  arrow: string
  format?: CatalogFormat
  commander?: string
  empty: string
  onLook(identity: string): void
  onMove(identity: string): void
  onCommander(identity: string): void
}) {
  return (
    <div className="edit-pane">
      <div className="edit-head">
        {header ?? <span className="edit-title">{title}</span>}
        <span className="edit-count">{count}</span>
      </div>
      <div className="edit-list">
        {entries.map((entry) => {
          const card = catalog.byId.get(entry.identity)
          const isCommander = commander === entry.identity
          return (
            <button
              key={entry.identity}
              className="edit-row"
              title={`Move one copy — ${title.toLowerCase()} to the other list`}
              onMouseEnter={() => onLook(entry.identity)}
              onClick={() => onMove(entry.identity)}
            >
              <span className="edit-n">{entry.count}</span>
              {/* the card's own title bar, as the builder's Titles view draws it */}
              <span className="edit-band">
                {card ? (
                  <CardTitle face={catalogFace(card)} />
                ) : (
                  <span className="edit-name">{entry.identity}</span>
                )}
              </span>
              {format?.requires_commander && (
                <span
                  role="button"
                  tabIndex={0}
                  className={`edit-cmdr${isCommander ? ' edit-cmdr-on' : ''}`}
                  title={isCommander ? 'The commander' : 'Make this the commander'}
                  onClick={(event) => {
                    event.stopPropagation()
                    onCommander(entry.identity)
                  }}
                  onKeyDown={(event) => {
                    if (event.key !== 'Enter' && event.key !== ' ') return
                    event.preventDefault()
                    event.stopPropagation()
                    onCommander(entry.identity)
                  }}
                >
                  ★
                </span>
              )}
              <span className="edit-arrow">{arrow}</span>
            </button>
          )
        })}
        {entries.length === 0 && <div className="edit-empty">{empty}</div>}
      </div>
    </div>
  )
}

export function DeckEditor({
  draft,
  catalog,
  format,
  name,
  onChange,
  onSave,
  onOpenBuilder,
  onClose,
}: {
  draft: DeckDraft
  catalog: Catalog
  format?: CatalogFormat
  name: string
  onChange(draft: DeckDraft): void
  onSave(): void
  onOpenBuilder(): void
  onClose(): void
}) {
  const side = sideEntries(draft)
  const [shown, setShown] = useState<string | undefined>(draft.entries[0]?.identity)
  const [pane, setPane] = useState<'side' | 'commander'>('side')

  const size = deckSize(draft)
  const advice = sizeAdvice(draft, format)
  // The pointer decides what is drawn beside the lists; before it has moved, the first card in
  // front of the player is a better answer than an empty pane.
  const looking = catalog.byId.get(shown ?? draft.entries[0]?.identity ?? '')

  const move = (identity: string, from: DeckList) => onChange(moved(draft, identity, from))

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
          <List
            title="In the deck"
            count={size}
            entries={draft.entries}
            catalog={catalog}
            arrow="→"
            {...(format === undefined ? {} : { format })}
            {...(draft.commander === undefined ? {} : { commander: draft.commander })}
            empty="Nothing here yet."
            onLook={setShown}
            onMove={(identity) => move(identity, 'main')}
            onCommander={(identity) =>
              onChange(withCommander(draft, draft.commander === identity ? undefined : identity))
            }
          />

          <div className="edit-pane edit-side">
            <div className="edit-head">
              <span className="edit-title">{looking?.name ?? 'Card'}</span>
            </div>
            <div className={`edit-preview card-${costTint(looking?.mana_cost)}`}>
              {looking && <Card face={catalogFace(looking)} />}
            </div>
          </div>

          <List
            title={pane === 'side' ? 'Beside it' : 'Commander'}
            header={
              <span className="seg edit-seg" role="radiogroup" aria-label="Pile">
                {PANES.map((option) => (
                  <button
                    key={option.id}
                    role="radio"
                    aria-checked={pane === option.id}
                    className={`seg-btn${pane === option.id ? ' seg-on' : ''}`}
                    onClick={() => setPane(option.id)}
                  >
                    {option.label}
                  </button>
                ))}
              </span>
            }
            count={
              pane === 'side'
                ? side.reduce((total, entry) => total + entry.count, 0)
                : draft.commander === undefined
                  ? 0
                  : 1
            }
            entries={
              pane === 'side'
                ? side
                : draft.commander === undefined
                  ? []
                  : [{ identity: draft.commander, count: 1 }]
            }
            catalog={catalog}
            // A card leaves the sideboard for the deck; the commander leaves by no longer being
            // the commander, and stays exactly where it is.
            arrow={pane === 'side' ? '←' : '✕'}
            empty={pane === 'side' ? 'Nothing beside the deck.' : 'No commander designated.'}
            onLook={setShown}
            onMove={(identity) =>
              pane === 'side' ? move(identity, 'side') : onChange(withCommander(draft, undefined))
            }
            onCommander={() => undefined}
          />
        </div>

        <div className="zone-foot">
          <span className="zone-hint">
            {advice ?? `${size} ${size === 1 ? 'card' : 'cards'} in the deck`} · the cards beside it
            are not sent to the server
          </span>
          <div className="zone-acts">
            <button className="action-done action-alt" onClick={onOpenBuilder}>
              Deck editor…
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
