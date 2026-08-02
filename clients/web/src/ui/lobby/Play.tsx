/**
 * Play: the tables, and the one you are at.
 *
 * **Which of the two is on screen is the server's answer, not a client-held phase** — a
 * `LobbyView` carrying a `room` is a table you are at, one without it is the directory. Joining
 * replaces the list with the table, because the server said so, and there is no state here that
 * could disagree with it.
 *
 * That is the other half of `docs/client-design.md` §9.4's distinction. Which *destination* you
 * are on is this client's answer and it is held in `App.tsx`; which *contract* you are on is the
 * server's and it is read off the view, here. Neither can be mistaken for the other, because
 * this component holds no state about which screen it is drawing.
 *
 * Every control is gated on `valid_commands`. The server publishes exactly which commands this
 * connection may currently send, so what is on screen is what it offered — the client never works
 * out that a room looks joinable, that a deck looks legal, or that it happens to be the host.
 */
import { useState } from 'react'

import { rejectionText, type Catalog, type DeckDraft } from './../../deck'
import { STARTER_DECKS } from './../../decks'
import { roster, tableLabel, tables } from './../../lobby'
import type {
  CatalogFormat,
  LobbyCommand,
  LobbyRejection,
  LobbyView,
  RoomConfig,
} from './../../protocol'
import { DeckPanel } from './DeckPanel'
import { SeatRoster } from './SeatRoster'
import { TableDirectory } from './TableDirectory'
import { TableForm } from './TableForm'

export function Play({
  view,
  catalog,
  format,
  draft,
  error,
  commands,
  send,
  onStarter,
  onBuild,
  onSubmitDeck,
}: {
  view: LobbyView
  catalog: Catalog
  /** The room's format, when the catalog described it. Its rules are quoted, never applied. */
  format?: CatalogFormat
  draft: DeckDraft
  error?: LobbyRejection
  commands: readonly string[]
  send(command: LobbyCommand): void
  onStarter(id: string): void
  onBuild(): void
  onSubmitDeck(): void
}) {
  const [creating, setCreating] = useState(false)
  const [editing, setEditing] = useState(false)

  const can = (command: string) => commands.includes(command)
  const room = view.room

  const create = (config: RoomConfig) => {
    send({ type: 'create_room', config })
    setCreating(false)
  }
  const update = (config: RoomConfig) => {
    send({ type: 'update_room', config })
    setEditing(false)
  }

  if (!room) {
    return (
      <>
        {/* A deck rejection belongs beside the deck it refused, and that is where the table
            screen shows it. Out here there is no deck panel to put it in, and the lobby's
            non-fatal errors are not only about decks — so it is said plainly instead of being
            dropped for want of somewhere to sit. */}
        {error && (
          <p role="alert" className="notice">
            The server refused that — {rejectionText(error, catalog)}
          </p>
        )}

        {creating ? (
          <section aria-label="New table" className="page">
            <header className="page__head">
              <h1>New table</h1>
            </header>
            <TableForm
              formats={catalog.formats}
              submitLabel="Create the table"
              onSubmit={create}
              onCancel={() => setCreating(false)}
            />
          </section>
        ) : (
          <TableDirectory
            entries={tables(view)}
            joinById={can('join_room')}
            canCreate={can('create_room')}
            onCreate={() => setCreating(true)}
            onReach={(entry) =>
              send(
                entry.reach === 'spectate_room'
                  ? { type: 'spectate_room', room_id: entry.roomId }
                  : { type: 'join_room', room_id: entry.roomId },
              )
            }
            onJoinById={(roomId) => send({ type: 'join_room', room_id: roomId })}
          />
        )}
      </>
    )
  }

  const rows = roster(room, view.you, catalog.aiNames)
  const isPrivate = room.config.visibility === 'private'

  return (
    <section aria-label="Table" className="page">
      <header className="page__head">
        <h1>{tableLabel(room)}</h1>
        <span className="chip">{room.config.game_setup}</span>
        {/* The one place a room id earns its space: an unlisted table is reachable only by the
            id its host passes on, so it is drawn where the host is, and nowhere else. */}
        {isPrivate && (
          <span className="chip chip--id">
            private <code>{room.room_id}</code>
          </span>
        )}
        <span className="page__controls">
          {can('update_room') && (
            <button type="button" onClick={() => setEditing((open) => !open)}>
              {editing ? 'Cancel the edit' : 'Edit table'}
            </button>
          )}
          {can('ready') && (
            <button
              type="button"
              className="page__lead"
              onClick={() => send({ type: 'ready', ready: true })}
            >
              Ready
            </button>
          )}
          {can('unready') && (
            <button type="button" onClick={() => send({ type: 'ready', ready: false })}>
              Not ready
            </button>
          )}
          {can('leave') && (
            <button type="button" onClick={() => send({ type: 'leave' })}>
              Leave
            </button>
          )}
        </span>
      </header>

      {editing && (
        <TableForm
          formats={catalog.formats}
          initial={room.config}
          submitLabel="Save the table"
          onSubmit={update}
          onCancel={() => setEditing(false)}
        />
      )}

      <div className="table-at">
        <SeatRoster
          rows={rows}
          aiOptions={catalog.ai}
          starters={STARTER_DECKS}
          canAddAi={can('add_ai')}
          canRemoveAi={can('remove_ai')}
          onSeatAi={(seat, kind, cards) => send({ type: 'add_ai', seat, kind, cards: [...cards] })}
          onRemoveAi={(seat) => send({ type: 'remove_ai', seat })}
        />

        <DeckPanel
          draft={draft}
          catalog={catalog}
          format={format}
          starters={STARTER_DECKS}
          rejection={error && rejectionText(error, catalog)}
          canSubmit={can('submit_deck')}
          onStarter={onStarter}
          onBuild={onBuild}
          onSubmit={onSubmitDeck}
        />
      </div>
    </section>
  )
}
