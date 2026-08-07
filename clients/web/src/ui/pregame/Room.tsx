/**
 * The table you are at, as its own screen (`docs/client-design.md` §9.5).
 *
 * Joining replaces the list rather than opening a panel inside it: topbar, the table's name and
 * how full it is, the rules strip, the seats, an action footer, and the same tabbed side panel
 * the lobby has.
 *
 * **A table's rules are chosen when it is made and shown where it is played.** The strip under
 * the title carries them plainly, and a rule that changes how the game plays is drawn in its own
 * colour. Three of them — the mulligan, the clock, and undo — are not on the wire; they are
 * drawn as what every table currently does, unpressable, rather than left out and having to be
 * designed back in the day the server carries them. **Undo is drawn as unavailable, not as a
 * rule that is on**: `RoomConfig` has no undo field, so a table that said `Undo allowed` was
 * telling a player a gameplay rule existed when nothing behind it did (issue #704). The green
 * "allowed" of §9.5 is what this becomes once #648 puts the fact on the wire.
 *
 * **A seat shows what its player brought, and both seats are drawn the same way** (§9.7): the deck's
 * colours, and the commander it was built around. Your own comes from the draft on this device;
 * everybody else's is the summary the server put on their `SeatView`. Neither is this screen
 * reading a decklist — nothing here can, and nothing here tries.
 *
 * **The two deck controls on your seat are the deck editor's, made small.** `Change` opens the same
 * loader the editor loads from, and `Edit` opens the sideboard line with a way through to the
 * editor itself; both are gated on `submit_deck` being advertised.
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
import { STARTER_DECKS } from './../../decks'
import { roster, tableLabel } from './../../lobby'
import type { LobbyCommand, LobbyRejection, LobbyView, RoomView } from './../../protocol'
import { rejectionText } from './../../deck'
import { AiPicker } from './Pickers'
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
  /** Opens the deck chooser, which is the same dialog the deck editor loads from. */
  onDeck(): void
  send(command: LobbyCommand): void
}) {
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
          {/* Not a table rule yet: the wire carries no undo field, so this states what is true of
              every table today rather than colouring a rule the server never sent (issue #704). */}
          <span className="fact">Undo unavailable</span>
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
                      // Your own seat is drawn from the draft on this device, which is ahead of
                      // anything the server has been told.
                      deck: {
                        name:
                          deckName ?? (draft.entries.length > 0 ? 'Your deck' : 'No deck chosen'),
                        colors,
                        ...(draft.commander === undefined ? {} : { commander: draft.commander }),
                      },
                    }
                  : row.decked
                    ? {
                        // Everybody else's is what the server said about theirs: the colours it
                        // is in and the commander it named, and never a card of it.
                        deck: {
                          name: 'Deck submitted',
                          colors: row.colors,
                          ...(row.commander === undefined ? {} : { commander: row.commander }),
                        },
                      }
                    : {})}
                {...(row.you && canDeck ? { onDeck, onEdit: onEditDeck } : {})}
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

      {seating !== undefined && (
        <AiPicker
          kinds={catalog.ai}
          decks={STARTER_DECKS}
          onClose={() => setSeating(undefined)}
          onSeat={(kind, cards, commander) =>
            send({
              type: 'add_ai',
              seat: seating,
              kind,
              cards: [...cards],
              ...(commander === undefined ? {} : { commander }),
            })
          }
        />
      )}
    </div>
  )
}
