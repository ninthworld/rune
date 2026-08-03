/**
 * The table you are at, as its own screen (`docs/client-design.md` §9.5).
 *
 * Joining replaces the list rather than opening a panel inside it: topbar, the table's name and
 * how full it is, the rules strip, the seats, an action footer, and the same tabbed side panel
 * the lobby has.
 *
 * **A table's rules are chosen when it is made and shown where it is played.** The strip under
 * the title carries them plainly, and a rule that changes how the game plays is drawn in its own
 * colour. Two of them — the mulligan and the clock — and undo are not on the wire; they are
 * drawn as what every table currently does, unpressable, rather than left out and having to be
 * designed back in the day the server carries them.
 *
 * **The footer is the board's action bar doing the same job before the game starts** (§6.5):
 * blue while the table is still waiting, green once every seat is ready, carrying the tally in
 * words and your `Ready`. It has no `Start game`, because the protocol has none: the room starts
 * when the server's own gate closes, and a button that could only ever be pressed after that had
 * already happened would be a decoration a player would blame for the wait.
 *
 * Everything interactive here is gated on `valid_commands` — leaving, readying, seating a bot,
 * unseating one, submitting a deck. A control missing is the server saying that step is not
 * available now, and this screen never works out that it is the host.
 */
import { useState } from 'react'

import { deckColors, formatOf, type Catalog, type DeckDraft } from './../../deck'
import { STARTER_DECKS, type StarterDeck } from './../../decks'
import { roster, tableLabel } from './../../lobby'
import type { LobbyCommand, LobbyRejection, LobbyView, RoomView } from './../../protocol'
import { rejectionText } from './../../deck'
import { AiPicker, DeckPicker } from './Pickers'
import { OpenSeat, Seat } from './Seat'
import { SidePanel } from './SidePanel'

export function Room({
  view,
  room,
  catalog,
  draft,
  deckName,
  error,
  commands,
  name,
  sideOpen,
  onSide,
  onSettings,
  onEditDeck,
  onDeck,
  send,
}: {
  view: LobbyView
  room: RoomView
  catalog: Catalog
  draft: DeckDraft
  /** What the deck on this device is called, or nothing when none has been chosen. */
  deckName?: string
  error?: LobbyRejection
  commands: readonly string[]
  name: string
  sideOpen: boolean
  onSide(open: boolean): void
  onSettings(): void
  onEditDeck(): void
  onDeck(deck: StarterDeck): void
  send(command: LobbyCommand): void
}) {
  const [picking, setPicking] = useState(false)
  const [seating, setSeating] = useState<number | undefined>(undefined)

  const rows = roster(room, view.you, catalog.aiNames)
  const format = formatOf(catalog, room.config.game_setup)
  const seats = room.config.seats
  const taken = rows.filter((row) => row.occupied).length
  const readyCount = rows.filter((row) => row.ready).length
  const you = rows.find((row) => row.you)
  const everyoneReady = taken > 0 && rows.every((row) => !row.occupied || row.ready)
  const full = taken >= seats

  const canReady = commands.includes('ready')
  const canAddAi = commands.includes('add_ai') && catalog.ai.length > 0
  const canRemoveAi = commands.includes('remove_ai')
  const canDeck = commands.includes('submit_deck')

  // The seats the server sent, then the holes: a room states its occupied seats and the config
  // states how many there are, so the difference is what is still open.
  const holes = Math.max(0, seats - rows.length)
  const colors = deckColors(draft, catalog)

  return (
    <div className={`lobby room${sideOpen ? '' : ' side-hidden'}`}>
      <div className="topbar lobby-topbar">
        <button className="view-btn" onClick={() => send({ type: 'leave' })}>
          ← Leave table
        </button>
        <span className="topbar-fill" />
        <span className="lobby-who">
          <b>{name}</b>
          <span className="lobby-server">{you ? 'seated' : 'watching'}</span>
        </span>
        <button className="settings-btn" title="Settings" onClick={onSettings}>
          ⚙
        </button>
        <button
          className="menu-btn"
          title="Chat and spectators"
          aria-expanded={sideOpen}
          onClick={() => onSide(!sideOpen)}
        >
          ☰
        </button>
      </div>

      <div className="lobby-main">
        <div className="lobby-head">
          <h1 className="lobby-title">{tableLabel(room)}</h1>
          <span className="lobby-tally">
            {taken} of {seats} seated
          </span>
        </div>

        {/* the table's own rules, on show rather than remembered */}
        <div className="filter-strip room-facts">
          <span className="fact">
            <b>{room.config.game_setup}</b>
          </span>
          <span className="fact">{seats} seats</span>
          <span className="fact">
            {room.config.visibility === 'private' ? 'Invite only' : 'Open to anyone'}
          </span>
          {format && format.min_deck_size > 0 && (
            <span className="fact">{format.min_deck_size} card minimum</span>
          )}
          <span className="fact fact-on">Undo allowed</span>
        </div>

        <div
          className={`seat-grid${format?.requires_commander ? ' cmdr-grid' : ''}`}
          style={{ '--seat-cols': Math.min(4, seats) } as React.CSSProperties}
          role="region"
          aria-label="Table"
        >
          {rows.map((row) =>
            // A seat nobody is in is a hole in the table, not a pane on it — whether the server
            // sent it as an empty seat or left it out of the list entirely.
            row.occupied ? (
              <Seat
                key={row.seat}
                row={row}
                catalog={catalog}
                {...(row.you
                  ? {
                      deck: {
                        name:
                          deckName ?? (draft.entries.length > 0 ? 'Your deck' : 'No deck chosen'),
                        colors,
                        ...(format?.requires_commander && draft.commander !== undefined
                          ? { commander: draft.commander }
                          : {}),
                      },
                    }
                  : {})}
                {...(row.you && canDeck
                  ? { onDeck: () => setPicking(true), onEdit: onEditDeck }
                  : {})}
                {...(canRemoveAi && row.ai !== undefined
                  ? { onRemove: () => send({ type: 'remove_ai', seat: row.seat }) }
                  : {})}
              />
            ) : (
              <OpenSeat
                key={row.seat}
                {...(canAddAi ? { onSeatAi: () => setSeating(row.seat) } : {})}
              />
            ),
          )}
          {Array.from({ length: holes }, (_, index) => {
            const seat = rows.length + index
            return (
              <OpenSeat key={seat} {...(canAddAi ? { onSeatAi: () => setSeating(seat) } : {})} />
            )
          })}
        </div>
      </div>

      <SidePanel
        open={sideOpen}
        tabs={[
          { id: 'chat', label: 'Chat', chat: true, empty: 'A table carries no chat yet.' },
          {
            id: 'watching',
            label: 'Watching',
            empty: 'Who is watching is not carried on the wire yet.',
          },
        ]}
      />

      {/* the board's action bar, doing the same job before the game starts */}
      <div className={`action-bar room-foot ${everyoneReady ? 'action-green' : ''}`}>
        <div className="action-text">
          <span className="action-prompt">
            {error
              ? rejectionText(error, catalog)
              : everyoneReady
                ? 'Everyone is ready'
                : `${readyCount} of ${taken} players ready`}
          </span>
          <span className="action-phase">
            {full
              ? 'Every seat is taken'
              : `${seats - taken} seat${seats - taken > 1 ? 's' : ''} still open`}
          </span>
        </div>
        <div className="action-btns">
          {you && canReady && (
            <button
              className={`action-done${you.ready ? ' action-alt' : ''}`}
              onClick={() => send({ type: 'ready', ready: !you.ready })}
            >
              {you.ready ? 'Not ready' : 'Ready'}
            </button>
          )}
        </div>
      </div>

      {picking && (
        <DeckPicker
          decks={STARTER_DECKS}
          {...(deckName !== undefined
            ? { current: STARTER_DECKS.find((deck) => deck.name === deckName)?.id }
            : {})}
          onClose={() => setPicking(false)}
          onPick={onDeck}
        />
      )}

      {seating !== undefined && (
        <AiPicker
          kinds={catalog.ai}
          decks={STARTER_DECKS}
          onClose={() => setSeating(undefined)}
          onSeat={(kind, cards) => send({ type: 'add_ai', seat: seating, kind, cards: [...cards] })}
        />
      )}
    </div>
  )
}
