/**
 * The WebSocket connection.
 *
 * One socket carries the whole session: lobby frames before the game, view frames after, and
 * the same socket for both. The server switches contracts on it without ceremony, which is why
 * `frame.ts` classifies every payload rather than the client tracking a mode.
 *
 * The transport is injected so tests drive a fake without a server or a browser. Nothing here
 * interprets a frame beyond decoding it.
 */
import { decodeFrame, type ServerFrame } from './frame'
import type { ClientMessage, LobbyCommand } from './protocol'

export type ConnectionStatus = 'connecting' | 'open' | 'closed' | 'error'

/** The subset of `WebSocket` this client uses, so a test can supply its own. */
export interface SocketLike {
  send(data: string): void
  close(): void
  onopen: ((event: unknown) => void) | null
  onclose: ((event: unknown) => void) | null
  onerror: ((event: unknown) => void) | null
  onmessage: ((event: { data: unknown }) => void) | null
}

export type SocketFactory = (url: string) => SocketLike

export interface ConnectionHandlers {
  onFrame(frame: ServerFrame): void
  onStatus(status: ConnectionStatus): void
}

export interface Connection {
  send(message: LobbyCommand | ClientMessage): void
  close(): void
}

const defaultFactory: SocketFactory = (url) => new WebSocket(url) as unknown as SocketLike

/**
 * The address of the server this client talks to.
 *
 * Defaults to the dev server's own host so a built client served from the same origin needs no
 * configuration, and falls back to the server's default local address when running from a file
 * or a different port during development.
 */
export function defaultServerUrl(): string {
  const configured = import.meta.env?.VITE_SAGE_SERVER
  if (typeof configured === 'string' && configured.length > 0) return configured
  if (typeof window !== 'undefined' && window.location?.protocol?.startsWith('http')) {
    const scheme = window.location.protocol === 'https:' ? 'wss' : 'ws'
    return `${scheme}://${window.location.hostname}:9000`
  }
  return 'ws://127.0.0.1:9000'
}

export function connect(
  url: string,
  handlers: ConnectionHandlers,
  factory: SocketFactory = defaultFactory,
): Connection {
  const socket = factory(url)
  handlers.onStatus('connecting')

  socket.onopen = () => handlers.onStatus('open')
  socket.onclose = () => handlers.onStatus('closed')
  socket.onerror = () => handlers.onStatus('error')
  socket.onmessage = (event) => {
    // Only text frames carry protocol messages; anything else is ignored, as the CLI does.
    if (typeof event.data !== 'string') return
    handlers.onFrame(decodeFrame(event.data))
  }

  return {
    send: (message) => socket.send(JSON.stringify(message)),
    close: () => socket.close(),
  }
}
