import { describe, expect, it, vi } from 'vitest'

import type { ServerFrame } from './frame'
import { connect, type ConnectionStatus, type SocketLike } from './socket'

/** A socket the test drives directly, so no server and no browser are involved. */
function fakeSocket() {
  const sent: string[] = []
  const socket: SocketLike = {
    send: (data) => sent.push(data),
    close: vi.fn(),
    onopen: null,
    onclose: null,
    onerror: null,
    onmessage: null,
  }
  return { socket, sent }
}

function harness() {
  const { socket, sent } = fakeSocket()
  const frames: ServerFrame[] = []
  const statuses: ConnectionStatus[] = []
  const connection = connect(
    'ws://test',
    { onFrame: (f) => frames.push(f), onStatus: (s) => statuses.push(s) },
    () => socket,
  )
  return { socket, sent, frames, statuses, connection }
}

describe('connection lifecycle', () => {
  it('reports connecting, then open', () => {
    const { socket, statuses } = harness()
    expect(statuses).toEqual(['connecting'])
    socket.onopen?.({})
    expect(statuses).toEqual(['connecting', 'open'])
  })

  it('reports a close and an error distinctly', () => {
    const { socket, statuses } = harness()
    socket.onerror?.({})
    socket.onclose?.({})
    expect(statuses.slice(-2)).toEqual(['error', 'closed'])
  })
})

describe('frames', () => {
  it('decodes and forwards a text frame', () => {
    const { socket, frames } = harness()
    socket.onmessage?.({ data: JSON.stringify({ you: 'p0', session: 's1' }) })
    expect(frames).toHaveLength(1)
    expect(frames[0]?.kind).toBe('lobby')
  })

  it('ignores a non-text frame rather than decoding it', () => {
    // Ping, pong, and binary frames carry no protocol message, exactly as the CLI treats them.
    const { socket, frames } = harness()
    socket.onmessage?.({ data: new ArrayBuffer(4) })
    expect(frames).toHaveLength(0)
  })

  it('survives an undecodable payload', () => {
    const { socket, frames } = harness()
    socket.onmessage?.({ data: 'not json at all' })
    expect(frames[0]?.kind).toBe('unknown')
  })
})

describe('sending', () => {
  it('holds a message sent before the socket opens, then flushes it', () => {
    // Every session's first message is `hello`, sent the moment the connection is created —
    // before any open event can have fired. A real WebSocket throws on a send while CONNECTING,
    // which took down the whole app: the throw escaped the effect and React never mounted.
    const { socket, sent, connection } = harness()
    connection.send({ type: 'hello' })
    expect(sent).toEqual([])

    socket.onopen?.({})
    expect(sent).toEqual(['{"type":"hello"}'])
  })

  it('flushes queued messages in order', () => {
    const { socket, sent, connection } = harness()
    connection.send({ type: 'hello' })
    connection.send({ type: 'ready', ready: true })
    socket.onopen?.({})
    expect(sent.map((s) => JSON.parse(s).type)).toEqual(['hello', 'ready'])
  })

  it('serializes a command to JSON', () => {
    const { socket, connection, sent } = harness()
    socket.onopen?.({})
    connection.send({ type: 'hello' })
    expect(sent).toEqual(['{"type":"hello"}'])
  })

  it('survives a socket that refuses the send', () => {
    // A real `WebSocket` throws if you send while it is CLOSING, and that window is open
    // between the call that closed it and the event that reports it — which is where a screen
    // re-mounting after a deliberate restart lands. The message is lost either way; taking the
    // page down with it is the part that must not happen.
    const { socket, connection } = harness()
    socket.onopen?.({})
    socket.send = () => {
      throw new Error('WebSocket is already in CLOSING or CLOSED state.')
    }
    expect(() => connection.send({ type: 'request_catalog' })).not.toThrow()
  })

  it('sends an in-game message on the same socket', () => {
    // The server switches this socket from the lobby contract to the in-game one without
    // reconnecting, so both message kinds go the same way.
    const { socket, connection, sent } = harness()
    socket.onopen?.({})
    connection.send({ type: 'choose_action', action_id: 'a0', token: 'tok' })
    expect(JSON.parse(sent[0] ?? '{}')).toEqual({
      type: 'choose_action',
      action_id: 'a0',
      token: 'tok',
    })
  })
})
