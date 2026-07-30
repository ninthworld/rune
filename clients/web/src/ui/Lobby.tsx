/**
 * The pre-game screen: one `LobbyView`, rendered.
 *
 * Every control here is gated on `valid_commands`. The server publishes exactly which commands
 * this connection may currently send, so the client offers those and no others — it never works
 * out for itself whether a room can be joined, a deck submitted, or a game started.
 */
import { useState } from 'react'

import { STARTER_DECKS } from './../decks'
import type { LobbyCommand, LobbyRejection, LobbyView } from './../protocol'

interface Props {
  view: LobbyView
  error?: LobbyRejection
  send(command: LobbyCommand): void
}

const DEFAULT_SETUP = 'starter-1v1'

export function Lobby({ view, error, send }: Props) {
  const [deckId, setDeckId] = useState(STARTER_DECKS[0]?.id ?? '')
  const [joinId, setJoinId] = useState('')
  const [name, setName] = useState('')

  const can = (command: string) => (view.valid_commands ?? []).includes(command)
  const deck = STARTER_DECKS.find((candidate) => candidate.id === deckId)

  return (
    <div className="lobby">
      <header>
        <h1>SAGE</h1>
        <p>
          You are {view.name ? `${view.name} (${view.you})` : view.you}
          {view.session && <> · session held</>}
        </p>
      </header>

      {error && (
        <p role="alert" className="notice">
          Deck rejected — {error.reason}
          {error.card && <> ({error.card})</>}
        </p>
      )}

      {can('set_name') && (
        <section aria-labelledby="name-heading">
          <h2 id="name-heading">Display name</h2>
          <input
            aria-label="Display name"
            value={name}
            onChange={(event) => setName(event.target.value)}
          />{' '}
          <button
            type="button"
            disabled={name.trim().length === 0}
            onClick={() => send({ type: 'set_name', name: name.trim() })}
          >
            Set name
          </button>
        </section>
      )}

      {view.room ? (
        <Room view={view} can={can} send={send} deck={deck} deckId={deckId} onDeck={setDeckId} />
      ) : (
        <>
          {can('create_room') && (
            <section aria-labelledby="create-heading">
              <h2 id="create-heading">New table</h2>
              <button
                type="button"
                onClick={() =>
                  send({ type: 'create_room', config: { seats: 2, game_setup: DEFAULT_SETUP } })
                }
              >
                Create a two-seat table
              </button>
            </section>
          )}

          <section aria-labelledby="directory-heading">
            <h2 id="directory-heading">Tables</h2>
            {(view.directory ?? []).length === 0 ? (
              <p>No public tables right now.</p>
            ) : (
              <ul>
                {(view.directory ?? []).map((room) => (
                  <li key={room.room_id}>
                    {room.config.name ?? room.room_id} — {room.config.game_setup} · {room.filled}/
                    {room.config.seats} seats · {room.state}
                    {room.spectators !== undefined && room.spectators > 0 && (
                      <> · {room.spectators} watching</>
                    )}{' '}
                    {can('join_room') && room.state === 'gathering' && (
                      <button
                        type="button"
                        onClick={() => send({ type: 'join_room', room_id: room.room_id })}
                      >
                        Join
                      </button>
                    )}
                    {can('spectate_room') && room.state === 'in_progress' && (
                      <button
                        type="button"
                        onClick={() => send({ type: 'spectate_room', room_id: room.room_id })}
                      >
                        Watch
                      </button>
                    )}
                  </li>
                ))}
              </ul>
            )}
            {can('join_room') && (
              <p>
                <input
                  aria-label="Table id"
                  placeholder="table id"
                  value={joinId}
                  onChange={(event) => setJoinId(event.target.value)}
                />{' '}
                <button
                  type="button"
                  disabled={joinId.trim().length === 0}
                  onClick={() => send({ type: 'join_room', room_id: joinId.trim() })}
                >
                  Join by id
                </button>
              </p>
            )}
          </section>
        </>
      )}
    </div>
  )
}

function Room({
  view,
  can,
  send,
  deck,
  deckId,
  onDeck,
}: {
  view: LobbyView
  can(command: string): boolean
  send(command: LobbyCommand): void
  deck?: (typeof STARTER_DECKS)[number]
  deckId: string
  onDeck(id: string): void
}) {
  const room = view.room
  if (!room) return null
  const seats = room.seats ?? []
  const mySeat = seats.find((seat) => seat.occupied_by === view.you)
  const openSeat = seats.find((seat) => seat.occupied_by === undefined)

  return (
    <section aria-labelledby="room-heading">
      <h2 id="room-heading">{room.config.name ?? room.room_id}</h2>
      <p>
        {room.config.game_setup} · {room.config.seats} seats · {room.config.visibility ?? 'public'}{' '}
        · id <code>{room.room_id}</code>
      </p>

      <ul>
        {seats.map((seat) => (
          <li key={seat.seat}>
            Seat {seat.seat + 1} — {seat.occupied_by ? (seat.name ?? seat.occupied_by) : 'open'}
            {seat.occupied_by === view.you && ' (you)'}
            {seat.ai && ` · AI (${seat.ai})`}
            {seat.decked && ' · decked'}
            {seat.ready && ' · ready'}
            {can('remove_ai') && seat.ai && (
              <>
                {' '}
                <button type="button" onClick={() => send({ type: 'remove_ai', seat: seat.seat })}>
                  Remove
                </button>
              </>
            )}
          </li>
        ))}
      </ul>

      {can('submit_deck') && (
        <div>
          <h3>Deck</h3>
          <label>
            Starter deck{' '}
            <select value={deckId} onChange={(event) => onDeck(event.target.value)}>
              {STARTER_DECKS.map((candidate) => (
                <option key={candidate.id} value={candidate.id}>
                  {candidate.name}
                </option>
              ))}
            </select>
          </label>
          {deck && <p>{deck.summary}</p>}
          <button
            type="button"
            disabled={!deck}
            onClick={() => deck && send({ type: 'submit_deck', cards: [...deck.cards] })}
          >
            Submit deck ({deck?.cards.length ?? 0} cards)
          </button>
        </div>
      )}

      {can('add_ai') && openSeat && deck && (
        <p>
          <button
            type="button"
            onClick={() =>
              send({
                type: 'add_ai',
                seat: openSeat.seat,
                kind: 'random',
                cards: [...deck.cards],
              })
            }
          >
            Seat an AI opponent in seat {openSeat.seat + 1}
          </button>
        </p>
      )}

      <p>
        {can('ready') && (
          <button type="button" onClick={() => send({ type: 'ready', ready: !mySeat?.ready })}>
            {mySeat?.ready ? 'Not ready' : 'Ready'}
          </button>
        )}{' '}
        {can('leave') && (
          <button type="button" onClick={() => send({ type: 'leave' })}>
            Leave
          </button>
        )}
      </p>
    </section>
  )
}
