/**
 * One seat at a table (`docs/client-design.md` §9.5).
 *
 * **A seat is a card, and the seats tile the way the board's do.** Each carries a ready dot, the
 * player's name, the deck, the state in words, and — on your own seat only — `Edit` and
 * `Change`. Your seat is ringed the way the active field is on the board, and ready is green in
 * the dot, the rim and the word, everywhere it appears.
 *
 * **An empty seat is a hole in the table, not a pane on it**: a dashed recess carrying `Open
 * seat` and the one thing that can fill it — an AI, from the kinds the catalog advertised.
 *
 * **A seat's deck is drawn as its colours**, and in a format that wants a commander it is drawn
 * as the card the deck is built around: the real card frame, ringed in the gold the command zone
 * wears, with the commander's name in gold beneath it. This is the one place in the client where
 * a card is used as an *identity* rather than as an object.
 *
 * What can be drawn about a deck differs by seat, and that is the wire's shape rather than a
 * choice: your own draft is on this device, so its colours and its commander are known here,
 * while another seat states only `decked`. That seat says what the server said about it and
 * nothing more.
 */
import { catalogFace } from './../../card-face'
import type { Catalog } from './../../deck'
import type { SeatRow } from './../../lobby'
import type { ManaColor } from './../../mana'
import { Card } from './../card/Card'
import { Pip } from './../card/Pips'

/** What this client knows about the deck in a seat. */
export interface SeatDeck {
  name: string
  colors: readonly ManaColor[]
  /** The card the deck is built around, in a format that asks for one. */
  commander?: string
}

export function Seat({
  row,
  deck,
  catalog,
  onDeck,
  onEdit,
  onRemove,
}: {
  row: SeatRow
  deck?: SeatDeck
  catalog: Catalog
  /** Offered on your own seat, when the server is taking decks. */
  onDeck?(): void
  onEdit?(): void
  /** Offered on an AI seat, when the server is offering to unseat it. */
  onRemove?(): void
}) {
  const commander = deck?.commander === undefined ? undefined : catalog.byId.get(deck.commander)

  return (
    <div className={`seat${row.you ? ' seat-mine' : ''}${row.ready ? ' seat-ready' : ''}`}>
      <div className="seat-head">
        <span className={`table-dot ${row.ready ? 'dot-ready' : 'dot-full'}`} />
        <span className="seat-name">{row.label}</span>
        {row.ai !== undefined && <span className="seat-badge">AI</span>}
        {onRemove && (
          <button className="seat-kick" title={`Remove ${row.label}`} onClick={onRemove}>
            ✕
          </button>
        )}
      </div>

      <div className="seat-body">
        <div className="seat-ident">
          {commander && (
            <span className="seat-cmdr">
              <Card face={catalogFace(commander)} />
            </span>
          )}
          {deck && deck.colors.length > 0 ? (
            <span className="seat-colors">
              {deck.colors.map((color) => (
                <Pip key={color} symbol={color.toUpperCase()} />
              ))}
            </span>
          ) : (
            <span className="seat-nodeck" />
          )}
        </div>
        {commander && <span className="seat-cmdr-name">{commander.name}</span>}
        <span className={`seat-deck-name${deck ? '' : ' seat-deck-none'}`}>
          {deck ? deck.name : row.decked ? 'Deck submitted' : 'No deck yet'}
        </span>
      </div>

      <div className="seat-foot">
        <span className={`seat-state${row.ready ? ' state-ready' : ''}`}>
          {row.ready ? '✓ Ready' : (row.awaiting ?? 'Not ready')}
        </span>
        {(onEdit || onDeck) && (
          <span className="seat-acts">
            {onEdit && (
              <button className="view-btn" onClick={onEdit}>
                Edit
              </button>
            )}
            {onDeck && (
              <button className="view-btn" onClick={onDeck}>
                Change
              </button>
            )}
          </span>
        )}
      </div>
    </div>
  )
}

/** A seat nobody is in: a hole in the table, and the one thing that can fill it. */
export function OpenSeat({ onSeatAi }: { onSeatAi?(): void }) {
  return (
    <div className="seat seat-open">
      <span className="seat-open-label">Open seat</span>
      {onSeatAi && (
        <button className="view-btn" onClick={onSeatAi}>
          Seat an AI opponent
        </button>
      )}
    </div>
  )
}
