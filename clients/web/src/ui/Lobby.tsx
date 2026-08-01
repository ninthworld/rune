/**
 * The pre-game screen: one `LobbyView`, arranged as a place rather than a form.
 *
 * This file composes and derives; the surfaces in `./lobby/` draw. Two screens live here because
 * the server decides which one you are on by whether it sent a `room` — a table you are at, or
 * the directory of tables you are not — and a client-held phase would be a second answer to a
 * question the view already answers.
 *
 * Every control is gated on `valid_commands`. The server publishes exactly which commands this
 * connection may currently send, so what is on screen is what it offered — the client never works
 * out that a room looks joinable, that a deck looks legal, or that it happens to be the host.
 *
 * The catalog is the one thing not carried on the view. It is reference data, requested per
 * socket (`docs/protocol.md`) and re-requested after a reconnect, and everything the pre-game UI
 * offers as a choice comes out of it: the formats a table can be created with, the seat counts
 * that format allows, the AI kinds that can be seated, and the cards a deck is built from. A
 * client that has not been handed one offers no format rather than guessing at ids.
 *
 * The deck draft is the only state that outlives a frame, and it is not game state: it is what
 * the player has typed into a builder, held so a rejected submission can be corrected and sent
 * again in the same room session, exactly as `docs/protocol.md` describes.
 */
import { useEffect, useMemo, useState } from 'react'

import { catalogFace } from './../card-face'
import {
  EMPTY_DECK,
  collect,
  expand,
  formatOf,
  readCatalog,
  rejectionText,
  withCard,
  withCommander,
  withoutCard,
  type DeckDraft,
} from './../deck'
import { STARTER_DECKS } from './../decks'
import { awaiting, roster, tableLabel, tables } from './../lobby'
import { list } from './../normalize'
import type {
  CatalogView,
  LobbyCommand,
  LobbyRejection,
  LobbyView,
  RoomConfig,
} from './../protocol'
import { CardInspector } from './CardInspector'
import { DeckBuilder } from './lobby/DeckBuilder'
import { DeckPanel } from './lobby/DeckPanel'
import { SeatRoster } from './lobby/SeatRoster'
import { TableDirectory } from './lobby/TableDirectory'
import { TableForm } from './lobby/TableForm'

export function Lobby({
  view,
  catalog: catalogView,
  error,
  epoch,
  send,
}: {
  view: LobbyView
  catalog?: CatalogView
  error?: LobbyRejection
  /** How many sockets this tab has opened; a new one has never been sent a catalog. */
  epoch: number
  send(command: LobbyCommand): void
}) {
  const [name, setName] = useState('')
  const [creating, setCreating] = useState(false)
  const [editing, setEditing] = useState(false)
  const [draft, setDraft] = useState<DeckDraft>(EMPTY_DECK)
  const [builderOpen, setBuilderOpen] = useState(false)
  const [inspecting, setInspecting] = useState<string | undefined>(undefined)

  // Asked once per socket, and again after a reconnect — the answer is a one-shot frame, so a
  // connection that never asks has no catalog and a connection that reconnected has a stale one.
  useEffect(() => {
    send({ type: 'request_catalog' })
  }, [epoch, send])

  const catalog = useMemo(() => readCatalog(catalogView), [catalogView])
  const commands = list(view.valid_commands)
  const can = (command: string) => commands.includes(command)

  const room = view.room
  const rows = room ? roster(room, view.you, catalog.aiNames) : []
  const format = room ? formatOf(catalog, room.config.game_setup) : undefined
  const inspected = inspecting === undefined ? undefined : catalog.byId.get(inspecting)

  const create = (config: RoomConfig) => {
    send({ type: 'create_room', config })
    setCreating(false)
  }
  const update = (config: RoomConfig) => {
    send({ type: 'update_room', config })
    setEditing(false)
  }
  const submitDeck = () =>
    send({
      type: 'submit_deck',
      cards: [...expand(draft)],
      ...(draft.commander !== undefined ? { commander: draft.commander } : {}),
    })

  return (
    <div className="lobby">
      <header className="lobby__head">
        <h1>SAGE</h1>
        <p>
          You are {view.name ? `${view.name} (${view.you})` : view.you}
          {view.session && <> · session held</>}
        </p>
        {can('set_name') && (
          <p className="lobby__name">
            <label>
              Display name{' '}
              <input
                aria-label="Display name"
                value={name}
                maxLength={32}
                onChange={(event) => setName(event.target.value)}
              />
            </label>{' '}
            <button
              type="button"
              disabled={name.trim().length === 0}
              onClick={() => send({ type: 'set_name', name: name.trim() })}
            >
              Set name
            </button>
          </p>
        )}
      </header>

      {room ? (
        <section aria-label="Table" className="room">
          <div className="room__head">
            <h2>{tableLabel(room)}</h2>
            <p className="room__line">
              {room.config.game_setup} · {room.config.seats} seats ·{' '}
              {room.config.visibility ?? 'public'} · id <code>{room.room_id}</code>
            </p>
            <p className="room__controls">
              {can('update_room') && (
                <button type="button" onClick={() => setEditing((open) => !open)}>
                  {editing ? 'Cancel the edit' : 'Edit table'}
                </button>
              )}{' '}
              {can('ready') && (
                <button type="button" onClick={() => send({ type: 'ready', ready: true })}>
                  Ready
                </button>
              )}
              {can('unready') && (
                <button type="button" onClick={() => send({ type: 'ready', ready: false })}>
                  Not ready
                </button>
              )}{' '}
              {can('leave') && (
                <button type="button" onClick={() => send({ type: 'leave' })}>
                  Leave
                </button>
              )}
            </p>
          </div>

          {editing && (
            <TableForm
              formats={catalog.formats}
              initial={room.config}
              submitLabel="Save the table"
              onSubmit={update}
              onCancel={() => setEditing(false)}
            />
          )}

          <SeatRoster
            rows={rows}
            waiting={awaiting(rows)}
            aiOptions={catalog.ai}
            starters={STARTER_DECKS}
            canAddAi={can('add_ai')}
            canRemoveAi={can('remove_ai')}
            onSeatAi={(seat, kind, cards) =>
              send({ type: 'add_ai', seat, kind, cards: [...cards] })
            }
            onRemoveAi={(seat) => send({ type: 'remove_ai', seat })}
          />

          <DeckPanel
            draft={draft}
            catalog={catalog}
            format={format}
            starters={STARTER_DECKS}
            rejection={error && rejectionText(error, catalog)}
            canSubmit={can('submit_deck')}
            builderOpen={builderOpen}
            onStarter={(id) => {
              const starter = STARTER_DECKS.find((candidate) => candidate.id === id)
              if (starter) setDraft(collect(starter.cards))
            }}
            onToggleBuilder={() => setBuilderOpen((open) => !open)}
            onSubmit={submitDeck}
          />

          {builderOpen && (
            <DeckBuilder
              catalog={catalog}
              format={format}
              draft={draft}
              onAdd={(identity) => setDraft((current) => withCard(current, identity))}
              onRemove={(identity) => setDraft((current) => withoutCard(current, identity))}
              onCommander={(identity) => setDraft((current) => withCommander(current, identity))}
              onInspect={setInspecting}
              onClose={() => setBuilderOpen(false)}
            />
          )}
        </section>
      ) : (
        <>
          {/* A deck rejection belongs beside the deck it refused, and that is where the room
              screen shows it. Out here there is no deck panel to put it in, and the lobby's
              non-fatal errors are not only about decks — so it is said plainly instead of
              being dropped for want of somewhere to sit. */}
          {error && (
            <p role="alert" className="notice">
              The server refused that — {rejectionText(error, catalog)}
            </p>
          )}

          {can('create_room') && (
            <section aria-labelledby="create-heading" className="create">
              <h2 id="create-heading">New table</h2>
              {creating ? (
                <TableForm
                  formats={catalog.formats}
                  submitLabel="Create the table"
                  onSubmit={create}
                  onCancel={() => setCreating(false)}
                />
              ) : (
                <button type="button" onClick={() => setCreating(true)}>
                  Create a table
                </button>
              )}
            </section>
          )}

          <TableDirectory
            entries={tables(view)}
            joinById={can('join_room')}
            onReach={(entry) =>
              send(
                entry.reach === 'spectate_room'
                  ? { type: 'spectate_room', room_id: entry.roomId }
                  : { type: 'join_room', room_id: entry.roomId },
              )
            }
            onJoinById={(roomId) => send({ type: 'join_room', room_id: roomId })}
          />
        </>
      )}

      {inspected && (
        <CardInspector face={catalogFace(inspected)} onClose={() => setInspecting(undefined)} />
      )}
    </div>
  )
}
