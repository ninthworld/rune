/**
 * The session hook: one socket, and the latest frame it delivered.
 *
 * What this holds is deliberately thin. The UI must be reconstructable from one view plus a
 * pending prompt (root `AGENTS.md`), so nothing here accumulates history or derives state
 * across messages — a new frame *replaces* its predecessor rather than merging into it.
 *
 * Three things are kept beyond the latest view, and each is either given by the server or
 * plainly not game state:
 *
 * - the session token, echoed on a later `hello` to reclaim a held-open seat;
 * - the most recent catalog, because it arrives on request as its own frame rather than on
 *   every view, and the deck builder needs it;
 * - the last lobby error, which is a one-shot notice the server sends alongside an otherwise
 *   unchanged lobby view, and which no view carries.
 */
import { useCallback, useEffect, useRef, useState } from 'react'

import type { ServerFrame } from './frame'
import type {
  CatalogView,
  ClientMessage,
  GameView,
  LobbyCommand,
  LobbyRejection,
  LobbyView,
  SpectatorView,
} from './protocol'
import {
  connect,
  defaultServerUrl,
  type Connection,
  type ConnectionStatus,
  type SocketFactory,
} from './socket'

export interface Session {
  status: ConnectionStatus
  lobby?: LobbyView
  game?: GameView
  spectator?: SpectatorView
  catalog?: CatalogView
  lobbyError?: LobbyRejection
  /** Frames this client could not classify — surfaced rather than hidden. */
  unknownFrames: number
  send(message: LobbyCommand | ClientMessage): void
}

export function useSession(url: string = defaultServerUrl(), factory?: SocketFactory): Session {
  const [status, setStatus] = useState<ConnectionStatus>('connecting')
  const [lobby, setLobby] = useState<LobbyView>()
  const [game, setGame] = useState<GameView>()
  const [spectator, setSpectator] = useState<SpectatorView>()
  const [catalog, setCatalog] = useState<CatalogView>()
  const [lobbyError, setLobbyError] = useState<LobbyRejection>()
  const [unknownFrames, setUnknownFrames] = useState(0)
  const connection = useRef<Connection>(null)

  const onFrame = useCallback((frame: ServerFrame) => {
    switch (frame.kind) {
      case 'lobby':
        setLobby(frame.view)
        break
      case 'game':
        // The hand-off: once a game view arrives this socket is on the in-game contract.
        setGame(frame.view)
        // A rejected deck's notice belongs to the pre-game screen it was raised on.
        setLobbyError(undefined)
        break
      case 'spectator':
        setSpectator(frame.view)
        break
      case 'catalog':
        setCatalog(frame.view)
        break
      case 'lobby_error':
        setLobbyError(frame.frame.lobby_error)
        break
      case 'unknown':
        // A newer server may send something this client has no concept of. Count it so the
        // screen can say so, and carry on — the connection must survive it.
        setUnknownFrames((n) => n + 1)
        break
    }
  }, [])

  useEffect(() => {
    const active = connect(url, { onFrame, onStatus: setStatus }, factory)
    connection.current = active
    // First contact carries no token; a reconnect flow would echo the stored one here.
    active.send({ type: 'hello' })
    return () => {
      connection.current = null
      active.close()
    }
  }, [url, factory, onFrame])

  const send = useCallback((message: LobbyCommand | ClientMessage) => {
    connection.current?.send(message)
  }, [])

  return { status, lobby, game, spectator, catalog, lobbyError, unknownFrames, send }
}
