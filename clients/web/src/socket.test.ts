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
  it('serializes a command to JSON', () => {
    const { connection, sent } = harness()
    connection.send({ type: 'hello' })
    expect(sent).toEqual(['{"type":"hello"}'])
  })

  it('sends an in-game message on the same socket', () => {
    // The server switches this socket from the lobby contract to the in-game one without
    // reconnecting, so both message kinds go the same way.
    const { connection, sent } = harness()
    connection.send({ type: 'choose_action', action_id: 'a0', token: 'tok' })
    expect(JSON.parse(sent[0] ?? '{}')).toEqual({
      type: 'choose_action',
      action_id: 'a0',
      token: 'tok',
    })
  })
})
