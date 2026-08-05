/**
 * The deck itself: columns of cards, in whichever reading the options bar is set to.
 *
 * **The deck is what fills the space; the other two piles share a column beside it.** A deck has
 * one commander and a sideboard that is small, and neither should cost the deck half its width —
 * so the aside shows one of them at a time, or neither, and folds away when it is showing
 * nothing. Which pile it is on is a device preference, not a fact about the deck.
 *
 * Double-clicking a card takes a copy out of whatever list it is in, which is the pool's gesture
 * read backwards; the picked card moves between the lists through the aside's own button.
 */
import type { DeckColumn, DeckView } from './../../builder'
import { catalogFace } from './../../card-face'
import type { DeckList } from './../../deck'
import type { CatalogCard } from './../../protocol'
import { Card } from './../card/Card'
import { CardTitle } from './../card/TitleBand'

/** Which pile the aside is showing, or nothing at all. */
export type DeckPile = 'commander' | 'side' | undefined

/**
 * The one card a click chose: which card it is, and **which copy of it**.
 *
 * A deck holds four of a land and they are four cards on the table, not one card drawn four
 * times — clicking the third has to light the third. The identity rides along because every
 * action taken on a pick is about the card, not about the copy.
 */
export interface Pick {
  identity: string
  /** Where this copy sits: its list, its column, and its place in that column. */
  at: string
}

function Held({
  card,
  at,
  view,
  picked,
  onPick,
  onLook,
  onRemove,
}: {
  card: CatalogCard
  /** This copy, told apart from the others of the same card. */
  at: string
  view: DeckView
  picked: boolean
  onPick(pick: Pick): void
  onLook(identity: string | undefined): void
  onRemove(identity: string): void
}) {
  const held = picked ? ' is-picked' : ''
  const on = {
    title: 'Double-click to take a copy out',
    onMouseEnter: () => onLook(card.functional_id),
    // The pointer leaving is the end of looking; what is drawn then is what was picked.
    onMouseLeave: () => onLook(undefined),
    onClick: () => onPick({ identity: card.functional_id, at }),
    onDoubleClick: () => onRemove(card.functional_id),
  }

  return view === 'titles' ? (
    <span className={`deck-line${held}`} {...on}>
      <CardTitle face={catalogFace(card)} />
    </span>
  ) : (
    <div className={`deck-stacked${held}`} {...on}>
      <Card face={catalogFace(card)} />
    </div>
  )
}

function Column({
  label,
  where,
  cards,
  view,
  picked,
  onPick,
  onLook,
  onRemove,
}: {
  label: string
  /** Which list this column belongs to, so a copy in the deck is not a copy in a pile. */
  where: string
  cards: readonly CatalogCard[]
  view: DeckView
  picked?: string
  onPick(pick: Pick): void
  onLook(identity: string | undefined): void
  onRemove(identity: string): void
}) {
  return (
    <div className="deck-col">
      <div className="deck-col-head">{label}</div>
      <div className="deck-stack">
        {cards.map((card, copy) => {
          const at = `${where}|${label}|${copy}`
          return (
            <Held
              key={`${card.functional_id}-${copy}`}
              card={card}
              at={at}
              view={view}
              picked={picked === at}
              onPick={onPick}
              onLook={onLook}
              onRemove={onRemove}
            />
          )
        })}
      </div>
    </div>
  )
}

export function DeckArea({
  columns,
  side,
  commander,
  view,
  picked,
  pile,
  onPick,
  onLook,
  onRemove,
  onMove,
}: {
  columns: readonly DeckColumn[]
  side: readonly DeckColumn[]
  /** The card the draft designates, if it has one. */
  commander?: CatalogCard
  view: DeckView
  /** The copy a click chose, wherever on this surface it was. */
  picked?: Pick
  /** Which pile is beside the deck, chosen in the options bar. */
  pile: DeckPile
  onPick(pick: Pick): void
  onLook(identity: string | undefined): void
  onRemove(identity: string, from: DeckList): void
  onMove(identity: string, to: 'commander' | 'side' | 'main'): void
}) {
  const shared = {
    view,
    ...(picked === undefined ? {} : { picked: picked.at }),
    onPick,
    onLook,
  }

  // Which way the picked card would go: out of the pile into the deck, or the other way. It is
  // the copy that says which, because the same card can be in both lists at once.
  const inPile = picked?.at.startsWith('pile|') === true

  return (
    <section className={`builder-deck deck-${view}`} aria-label="The deck">
      <div className="deck-columns">
        {columns.map((column) => (
          <Column
            key={column.label}
            label={column.label}
            where="deck"
            cards={column.cards}
            onRemove={(identity) => onRemove(identity, 'main')}
            {...shared}
          />
        ))}
        {columns.length === 0 && (
          <div className="zone-empty">Double-click a card above to put it in.</div>
        )}
      </div>

      {pile !== undefined && (
        <aside className="deck-aside" aria-label="Piles">
          <div className="deck-aside-head">
            <span className="deck-aside-title">
              {pile === 'commander' ? 'Commander' : 'Sideboard'}
            </span>
            {picked !== undefined && (
              <button
                className="action-done deck-aside-move"
                title={inPile ? 'Send it back to the deck' : 'Send it here'}
                onClick={() =>
                  onMove(
                    picked.identity,
                    inPile ? 'main' : pile === 'commander' ? 'commander' : 'side',
                  )
                }
              >
                {inPile ? '← Deck' : 'Here →'}
              </button>
            )}
          </div>

          <div className="deck-aside-body">
            {pile === 'commander' ? (
              commander ? (
                <Column
                  label={commander.name}
                  where="pile"
                  cards={[commander]}
                  onRemove={(identity) => onRemove(identity, 'main')}
                  {...shared}
                />
              ) : (
                <div className="zone-empty">No commander designated.</div>
              )
            ) : side.length > 0 ? (
              side.map((column) => (
                <Column
                  key={`side-${column.label}`}
                  label={column.label}
                  where="pile"
                  cards={column.cards}
                  onRemove={(identity) => onRemove(identity, 'side')}
                  {...shared}
                />
              ))
            ) : (
              <div className="zone-empty">Nothing beside the deck.</div>
            )}
          </div>
        </aside>
      )}
    </section>
  )
}
