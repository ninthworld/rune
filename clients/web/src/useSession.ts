/**
 * The session hook: one socket, and the latest frame it delivered.
 *
 * What this holds is deliberately thin. The UI must be reconstructable from one view plus a
 * pending prompt (root `AGENTS.md`), so nothing here accumulates history or derives state
 * across messages — a new frame *replaces* its predecessor rather than merging into it.
 *
 * Four things are kept beyond the latest view, and each is either given by the server or
 * plainly not game state:
 *
 * - the session token, echoed on a later `hello` to reclaim a held-open seat;
 * - the most recent catalog, because it arrives on request as its own frame rather than on
 *   every view, and the deck builder needs it;
 * - the last lobby error, which is a one-shot notice the server sends alongside an otherwise
 *   unchanged lobby view, and which no view carries;
 * - the **epoch**: how many sockets this tab has opened. Not state about the game — state about
 *   the transport, and the signal a screen needs to drop anything it was waiting on, since an
 *   `action_ack` is dropped server-side when a seat reconnects (`docs/protocol.md`).
 *
 * # Reconnecting
 *
 * A dropped socket is retried on its own, because the server holds a seat open across a
 * disconnect and the player should get it back without knowing that. Each attempt opens a fresh
 * socket and says `hello` with the stored token, which is what proves ownership of the held seat;
 * the server answers by putting this connection back on whatever contract that seat is on — a
 * lobby view, or the game it is in the middle of.
 *
 * **The token is claimed once and then defended.** A fresh socket is issued its own identity
 * before `hello` is read, so a lobby frame carrying a *different* session arrives routinely
 * during a reconnect and means nothing about whether the claim succeeded. Adopting it would throw
 * away the token that owns the seat. So a lobby frame's session is taken while this tab has no
 * game — where it is either the first identity or the answer to a claim that failed and the tab
 * genuinely is that new session — and ignored once a game has arrived, where the only token worth
 * keeping is the one holding the seat.
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
  /** How many sockets this tab has opened. A change means everything in flight was lost. */
  epoch: number
  /** Frames this client could not classify — surfaced rather than hidden. */
  unknownFrames: number
  send(message: LobbyCommand | ClientMessage): void
  /**
   * Give up this session and start a new one.
   *
   * The way out of a game: an in-game socket speaks the game contract for the life of that game
   * and has no "leave" to send, so returning to the lobby means forgetting the token that holds
   * the seat and connecting as somebody new.
   */
  restart(): void
}

/** Per **tab**, as `docs/protocol.md` specifies: two tabs are two players, not one. */
const TOKEN_KEY = 'sage.session'

/**
 * The room this tab is watching, if it is watching one.
 *
 * A seat is held open across a disconnect and a **spectator is not**: it owns no seat, so the
 * server drops it from the room's roster and says so plainly — "reconnect for a spectator is
 * `spectate_room`, which reconstructs the whole public board from its first `SpectatorView`"
 * (`lobby/connection.rs`). So the one thing a reconnecting spectator has to carry is *which room
 * it was watching*, which is a fact about this conversation rather than about the game — exactly
 * what the token beside it is, and kept the same way and in the same place, per tab.
 *
 * Without it a dropped socket leaves a spectator on a board that has stopped moving, with the
 * server's lobby view arriving behind it and nothing to put the two together.
 */
const SPECTATING_KEY = 'sage.spectating'

const readTab = (key: string): string | undefined => {
  try {
    return window.sessionStorage.getItem(key) ?? undefined
  } catch {
    // Storage can be unavailable (a sandboxed frame, a hardened browser). A tab that cannot
    // remember its token still plays; it just cannot reclaim its seat.
    return undefined
  }
}

const writeTab = (key: string, value: string | undefined): void => {
  try {
    if (value === undefined) window.sessionStorage.removeItem(key)
    else window.sessionStorage.setItem(key, value)
  } catch {
    /* see `readTab` */
  }
}

const readToken = (): string | undefined => readTab(TOKEN_KEY)
const writeToken = (token: string | undefined): void => writeTab(TOKEN_KEY, token)

/**
 * The token this tab holds, if any.
 *
 * Read outside the hook by the connect screen's gate: a tab carrying a token has already been
 * issued an identity and is owed whatever seat that identity holds, so it is put back on the
 * screen the server answers with rather than being asked who it is again.
 */
export const heldSession = (): string | undefined => readToken()

/** How long to wait before the nth retry: quick at first, then backing off to eight seconds. */
const retryDelay = (attempt: number): number => Math.min(8000, 500 * 2 ** Math.min(attempt, 4))

export function useSession(url: string = defaultServerUrl(), factory?: SocketFactory): Session {
  const [status, setStatus] = useState<ConnectionStatus>('connecting')
  const [lobby, setLobby] = useState<LobbyView>()
  const [game, setGame] = useState<GameView>()
  const [spectator, setSpectator] = useState<SpectatorView>()
  const [catalog, setCatalog] = useState<CatalogView>()
  const [lobbyError, setLobbyError] = useState<LobbyRejection>()
  const [unknownFrames, setUnknownFrames] = useState(0)
  const [epoch, setEpoch] = useState(0)
  const connection = useRef<Connection>(null)
  // Read through refs inside the frame handler so it never re-subscribes the socket.
  const token = useRef<string | undefined>(readToken())
  const seated = useRef(false)
  // The room this tab is watching, and whether the *current* socket has already asked to watch
  // it again. Asking once per socket is what keeps a room that has since ended — which answers
  // with an error and an unchanged lobby view — from being asked forever.
  const watching = useRef<string | undefined>(readTab(SPECTATING_KEY))
  const asked = useRef(false)

  const onFrame = useCallback((frame: ServerFrame) => {
    switch (frame.kind) {
      case 'lobby':
        setLobby(frame.view)
        if (frame.view.session !== undefined && !seated.current) {
          token.current = frame.view.session
          writeToken(frame.view.session)
        }
        // A lobby view on a tab that was watching is the server having dropped a spectator it
        // was holding no seat for. Ask again, with the board still on screen behind it: the next
        // `SpectatorView` is a whole public game, so resuming is indistinguishable from joining.
        if (watching.current !== undefined && !seated.current && !asked.current) {
          asked.current = true
          connection.current?.send({ type: 'spectate_room', room_id: watching.current })
        }
        break
      case 'game':
        // The hand-off: once a game view arrives this socket is on the in-game contract, and
        // the token that got here is the one that owns the seat.
        seated.current = true
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
        // The only command this tab sent on this socket was the request to watch again, so an
        // error on it is that request's: the room has ended, or is no longer one that can be
        // watched. Stop holding a board that will never move again and let the lobby through.
        if (watching.current !== undefined && asked.current) {
          watching.current = undefined
          writeTab(SPECTATING_KEY, undefined)
          setSpectator(undefined)
        }
        break
      case 'unknown':
        // A newer server may send something this client has no concept of. Count it so the
        // screen can say so, and carry on — the connection must survive it.
        setUnknownFrames((n) => n + 1)
        break
    }
  }, [])

  useEffect(() => {
    let closed = false
    let retry: ReturnType<typeof setTimeout> | undefined
    let attempt = 0
    let active: Connection | undefined

    const open = () => {
      active = connect(
        url,
        {
          onFrame,
          onStatus: (next) => {
            if (closed) return
            setStatus(next)
            if (next === 'open') attempt = 0
            // A dropped socket is not a dead end: the seat is held open server-side, so try
            // again rather than telling the player to reload.
            if (next === 'closed' || next === 'error') {
              if (retry === undefined) {
                retry = setTimeout(() => {
                  retry = undefined
                  attempt += 1
                  open()
                }, retryDelay(attempt))
              }
            }
          },
        },
        factory,
      )
      connection.current = active
      // A fresh socket has asked this server nothing yet, which is what makes one resume
      // attempt per socket exactly one attempt.
      asked.current = false
      // The token is what proves this connection owns a held-open seat; the first ever `hello`
      // has none and is issued a fresh identity.
      active.send({ type: 'hello', ...(token.current ? { token: token.current } : {}) })
      setEpoch((n) => n + 1)
    }

    open()
    return () => {
      closed = true
      if (retry !== undefined) clearTimeout(retry)
      connection.current = null
      active?.close()
    }
  }, [url, factory, onFrame])

  const send = useCallback((message: LobbyCommand | ClientMessage) => {
    // Which room this tab asked to watch is noted as it goes past rather than being threaded
    // down to the button that asks: the lobby row sends the command it was advertised
    // (`lobby.ts`) and knows nothing about reconnecting, which is where this belongs.
    if ('type' in message && message.type === 'spectate_room') {
      watching.current = message.room_id
      writeTab(SPECTATING_KEY, message.room_id)
      asked.current = true
    }
    connection.current?.send(message)
  }, [])

  const restart = useCallback(() => {
    token.current = undefined
    seated.current = false
    watching.current = undefined
    writeTab(SPECTATING_KEY, undefined)
    writeToken(undefined)
    setGame(undefined)
    setSpectator(undefined)
    setLobby(undefined)
    // Closing is enough to start over: the retry above reopens with no token, and the server
    // answers a token-less `hello` with a fresh session and its lobby.
    connection.current?.close()
  }, [])

  return {
    status,
    lobby,
    game,
    spectator,
    catalog,
    lobbyError,
    epoch,
    unknownFrames,
    send,
    restart,
  }
}
