/**
 * Everything before a game: the lobby, the table room, and the state they share.
 *
 * This file composes and derives; the surfaces beside it draw. What lives here is what both
 * screens need — the catalog, and the deck this device has chosen — because the room submits the
 * deck the editor builds and a draft held in either of them would be lost the moment the player
 * walked to the other one.
 *
 * **Which of the two screens is on is the server's answer, not a client-held phase.** A
 * `LobbyView` carrying a `room` is a table you are at; one without it is the directory. Nothing
 * here remembers which it was.
 *
 * The catalog is the one thing not carried on the view. It is reference data, requested per
 * socket (`docs/protocol.md`) and re-requested after a reconnect, and everything the pre-game UI
 * offers as a choice comes out of it: the formats a table can be created with, the seat counts
 * that format allows, the AI kinds that can be seated, and the cards a deck is built from. A
 * client that has not been handed one offers no format rather than guessing at ids.
 *
 * The name is the other thing sent from here, and it is sent **per socket**. A name is a device
 * preference chosen in setup (§9.3); a connection is what carries it to a server; and a reconnect
 * is a new connection, which is why this re-announces on every epoch rather than once ever.
 */
import { useEffect, useMemo, useRef, useState } from 'react'

import { collect, EMPTY_DECK, expand, formatOf, readCatalog, type DeckDraft } from './../../deck'
import { list } from './../../normalize'
import type { CatalogView, LobbyCommand, LobbyRejection, LobbyView } from './../../protocol'
import type { StarterDeck } from './../../decks'
import { deleteDeck, draftOf, savedDecks, type SavedDeck } from './../../deck-store'
import { parseDck, resolveDeck } from './../../dck'
import { DeckBuilder } from './../deck/DeckBuilder'
import { LoadDeck } from './../deck/DeckFiles'
import { DeckEditor } from './DeckEditor'
import { CreateTable } from './CreateTable'
import { Lobby } from './Lobby'
import { Room } from './Room'

/** Absent in a browser with storage disabled, which is a normal way to run. */
const deviceStorage = (): Storage | undefined => {
  try {
    return globalThis.localStorage
  } catch {
    return undefined
  }
}

export function Pregame({
  view,
  catalog: catalogView,
  error,
  epoch,
  name,
  server,
  onSettings,
  onDisconnect,
  send,
}: {
  view?: LobbyView
  catalog?: CatalogView
  error?: LobbyRejection
  /** How many sockets this tab has opened; a new one has never been sent a catalog or a name. */
  epoch: number
  /** The name this device connected as, and the address it connected to. */
  name: string
  server: string
  onSettings(): void
  onDisconnect(): void
  send(command: LobbyCommand): void
}) {
  const [draft, setDraft] = useState<DeckDraft>(EMPTY_DECK)
  const [deckName, setDeckName] = useState<string | undefined>(undefined)
  const [editing, setEditing] = useState(false)
  const [creating, setCreating] = useState(false)
  const [building, setBuilding] = useState(false)
  // Choosing a deck is the same dialog wherever it is opened from — the lobby, or a seat.
  const [choosing, setChoosing] = useState(false)
  const [saved, setSaved] = useState<readonly SavedDeck[]>(() => savedDecks(deviceStorage()))
  const [missing, setMissing] = useState<readonly string[]>([])
  const [sideOpen, setSideOpen] = useState(() => window.innerWidth > 900)

  // Asked once per socket, and again after a reconnect — the answer is a one-shot frame, so a
  // connection that never asks has no catalog and a connection that reconnected has a stale one.
  useEffect(() => {
    send({ type: 'request_catalog' })
  }, [epoch, send])

  const catalog = useMemo(() => readCatalog(catalogView), [catalogView])
  const commands = useMemo(() => list(view?.valid_commands), [view?.valid_commands])

  // Said once per socket per name. The ref is what makes it once: `valid_commands` is a fresh
  // array on every frame, so the effect runs often and the send must not.
  const announced = useRef<string>('')
  useEffect(() => {
    const said = `${epoch}:${name}`
    if (name.length === 0 || !commands.includes('set_name') || announced.current === said) return
    announced.current = said
    send({ type: 'set_name', name })
  }, [epoch, name, commands, send])

  const room = view?.room
  const format = room ? formatOf(catalog, room.config.game_setup) : undefined

  const submit = (deck: DeckDraft) =>
    send({
      type: 'submit_deck',
      cards: [...expand(deck)],
      ...(deck.commander !== undefined ? { commander: deck.commander } : {}),
    })

  /**
   * A deck arriving from anywhere — a starter, this device's storage, a file, the builder.
   *
   * **It is submitted only at a table**, because `submit_deck` is a command a room advertises and
   * nowhere else. Choosing a deck in the lobby is choosing what you will sit down with.
   */
  const applyDeck = (next: DeckDraft, called: string, gone: readonly string[] = []) => {
    setDraft(next)
    setDeckName(called)
    setMissing(gone)
    setChoosing(false)
    if (room) submit(next)
  }

  // A deck that names a commander is submitted with it; dropping the designation here would send
  // a commander deck as an ordinary one and let the server reject it for our omission.
  const chooseDeck = (deck: StarterDeck) =>
    applyDeck(collect(deck.cards, deck.commander), deck.name)

  if (!view) {
    return (
      <div className="lobby">
        <div className="topbar lobby-topbar">
          <span className="topbar-fill" />
          <button className="settings-btn" title="Settings" onClick={onSettings}>
            ⚙
          </button>
        </div>
        <div className="lobby-main">
          <div className="zone-empty">Waiting for the server’s lobby.</div>
        </div>
      </div>
    )
  }

  return (
    <>
      {building ? (
        <DeckBuilder
          draft={draft}
          catalog={catalog}
          onDraft={setDraft}
          name={name}
          server={server}
          // The way out is back to wherever the builder was opened from, and a deck changed at a
          // table is submitted on the way — the table is holding a seat for whatever it is now.
          back={room ? '← Table' : '← Lobby'}
          onBack={() => {
            if (room) submit(draft)
            setBuilding(false)
          }}
          onSettings={onSettings}
        />
      ) : room ? (
        <Room
          view={view}
          room={room}
          catalog={catalog}
          draft={draft}
          {...(deckName === undefined ? {} : { deckName })}
          {...(error === undefined ? {} : { error })}
          commands={commands}
          name={name}
          sideOpen={sideOpen}
          onSide={setSideOpen}
          onSettings={onSettings}
          onEditDeck={() => setEditing(true)}
          onDeck={() => setChoosing(true)}
          send={send}
        />
      ) : (
        <Lobby
          view={view}
          catalog={catalog}
          name={name}
          server={server}
          sideOpen={sideOpen}
          onSide={setSideOpen}
          onSettings={onSettings}
          onDisconnect={onDisconnect}
          onCreate={() => setCreating(true)}
          onDecks={() => setBuilding(true)}
          send={send}
        />
      )}

      {creating && (
        <CreateTable
          catalog={catalog}
          title="New table"
          submit="Create the table"
          onClose={() => setCreating(false)}
          onSubmit={(config) => {
            send({ type: 'create_room', config })
            setCreating(false)
          }}
        />
      )}

      {choosing && (
        <LoadDeck
          saved={saved}
          onSaved={(deck) => applyDeck(draftOf(deck), deck.name)}
          onStarter={chooseDeck}
          onFile={(file, text) => {
            const { draft: loaded, missing: gone } = resolveDeck(parseDck(text), catalog)
            applyDeck(loaded, file, gone)
          }}
          onDelete={(gone) => setSaved(deleteDeck(deviceStorage(), gone))}
          onClose={() => setChoosing(false)}
        />
      )}

      {editing && (
        <DeckEditor
          draft={draft}
          catalog={catalog}
          {...(format === undefined ? {} : { format })}
          name={deckName ?? 'Your deck'}
          onChange={(next) => {
            setDraft(next)
            setDeckName((current) => current ?? 'Your deck')
          }}
          onSave={() => submit(draft)}
          onOpenBuilder={() => {
            setEditing(false)
            setBuilding(true)
          }}
          onClose={() => setEditing(false)}
        />
      )}

      {missing.length > 0 && (
        <p role="status" className="notice">
          Not in this catalog, so they were left out: {missing.join(', ')}.{' '}
          <button className="view-btn" onClick={() => setMissing([])}>
            Dismiss
          </button>
        </p>
      )}
    </>
  )
}
