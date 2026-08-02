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
 * The address a `?server=` query parameter names, if this page was given one.
 *
 * Separated from the resolution below so the connect screen can tell an address that was
 * *configured for this page load* from one that is merely the default: an explicit parameter
 * outranks even what this device chose last (`connect.ts`), because it is what points one built
 * bundle at a staging server or a stub.
 */
export function serverUrlOverride(): string | undefined {
  if (typeof window === 'undefined' || !window.location?.search) return undefined
  return new URLSearchParams(window.location.search).get('server') ?? undefined
}

/**
 * The address of the server this client talks to, in precedence order:
 *
 * 1. a `?server=` query parameter — point a built client at another host without rebuilding;
 * 2. `VITE_SAGE_SERVER`, baked in at build time;
 * 3. the page's own host on the server's default port, so a client served from the same origin
 *    needs no configuration at all;
 * 4. the server's default local address, for a page opened from a file.
 *
 * The query parameter is ordinary configuration, not a test backdoor: it selects an address and
 * grants no capability the socket does not already have. It is what lets one built bundle be
 * pointed at a local server, a staging one, or a stub.
 *
 * This is the address a client that was never asked would use. What a player *chose* on the
 * connect screen is `connect.ts`, which extends this order rather than replacing it.
 */
export function defaultServerUrl(): string {
  const override = serverUrlOverride()
  if (override) return override
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

  // A real `WebSocket` throws if you send while it is still CONNECTING, and the first message
  // of every session — `hello` — is sent the moment the connection is created, before any open
  // event can have fired. Queue until open, then flush in order.
  let open = false
  const pending: string[] = []

  socket.onopen = () => {
    open = true
    for (const message of pending) socket.send(message)
    pending.length = 0
    handlers.onStatus('open')
  }
  socket.onclose = () => {
    open = false
    handlers.onStatus('closed')
  }
  socket.onerror = () => handlers.onStatus('error')
  socket.onmessage = (event) => {
    // Only text frames carry protocol messages; anything else is ignored, as the CLI does.
    if (typeof event.data !== 'string') return
    handlers.onFrame(decodeFrame(event.data))
  }

  return {
    send: (message) => {
      const json = JSON.stringify(message)
      if (!open) {
        pending.push(json)
        return
      }
      try {
        socket.send(json)
      } catch {
        // A real `WebSocket` throws if you send while it is CLOSING, and a socket can enter that
        // state between the call that closed it and the `close` event that reports it — which is
        // exactly the window a screen re-mounting after a deliberate restart sends in. The
        // message is lost either way; what matters is that losing it does not take the page down
        // with it. Everything a fresh connection needs is re-sent per epoch (`useSession.ts`).
      }
    },
    close: () => socket.close(),
  }
}
