/**
 * The pre-game content of the shell: the state it shares, and which destination draws it.
 *
 * This file composes and derives; the surfaces in `./lobby/` draw. What lives here is everything
 * two destinations both need — the catalog, and the deck draft — because Play submits the deck
 * that Decks builds, and a draft held in either of them would be lost the moment a player walked
 * to the other one.
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
 *
 * The name is the other thing sent from here, and it is sent **per socket**. A name is a device
 * preference chosen in setup (§9.3); a connection is what carries it to a server; and a reconnect
 * is a new connection, which is why this re-announces on every epoch rather than once ever. It is
 * still `set_name`, the command this client already sent — no wire change, and only ever when the
 * server is currently offering it.
 */
import { useEffect, useMemo, useRef, useState } from 'react'

import { catalogFace } from './../card-face'
import {
  EMPTY_DECK,
  collect,
  expand,
  formatOf,
  readCatalog,
  withCard,
  withCommander,
  withoutCard,
  type DeckDraft,
} from './../deck'
import { STARTER_DECKS } from './../decks'
import { list } from './../normalize'
import type { CatalogView, LobbyCommand, LobbyRejection, LobbyView } from './../protocol'
import type { Destination } from './../shell'
import { CardInspector } from './CardInspector'
import { Decks } from './lobby/Decks'
import { Play } from './lobby/Play'

export function Lobby({
  view,
  catalog: catalogView,
  error,
  epoch,
  name,
  destination,
  onDestination,
  send,
}: {
  view?: LobbyView
  catalog?: CatalogView
  error?: LobbyRejection
  /** How many sockets this tab has opened; a new one has never been sent a catalog or a name. */
  epoch: number
  /** The name this device connected as, or empty. */
  name: string
  destination: Destination
  onDestination(destination: Destination): void
  send(command: LobbyCommand): void
}) {
  const [draft, setDraft] = useState<DeckDraft>(EMPTY_DECK)
  const [inspecting, setInspecting] = useState<string | undefined>(undefined)

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
  const inspected = inspecting === undefined ? undefined : catalog.byId.get(inspecting)

  return (
    <>
      {destination === 'decks' ? (
        <Decks
          catalog={catalog}
          format={format}
          draft={draft}
          onAdd={(identity) => setDraft((current) => withCard(current, identity))}
          onRemove={(identity) => setDraft((current) => withoutCard(current, identity))}
          onCommander={(identity) => setDraft((current) => withCommander(current, identity))}
          onInspect={setInspecting}
          onDone={() => onDestination('play')}
        />
      ) : view ? (
        <Play
          view={view}
          catalog={catalog}
          format={format}
          draft={draft}
          error={error}
          commands={commands}
          send={send}
          onStarter={(id) => {
            const starter = STARTER_DECKS.find((candidate) => candidate.id === id)
            if (starter) setDraft(collect(starter.cards))
          }}
          onBuild={() => onDestination('decks')}
          onSubmitDeck={() =>
            send({
              type: 'submit_deck',
              cards: [...expand(draft)],
              ...(draft.commander !== undefined ? { commander: draft.commander } : {}),
            })
          }
        />
      ) : (
        <div className="page">
          <p className="page__pending">Waiting for the server’s lobby.</p>
        </div>
      )}

      {inspected && (
        <CardInspector face={catalogFace(inspected)} onClose={() => setInspecting(undefined)} />
      )}
    </>
  )
}
