/**
 * Where this client connects, and who it says it is.
 *
 * Two boundaries are worth pinning. The **precedence** — explicit configuration, then what this
 * device chose, then the default `socket.ts` resolves — because a remembered address quietly
 * outranking a `?server=` parameter would make one built bundle impossible to point anywhere.
 * And the **fallbacks**, because every one of them is a device that cannot remember: no storage,
 * unreadable storage, a stored address that no longer matches any entry. None of those is an
 * error, and all of them still connect.
 */
import { describe, expect, it } from 'vitest'

import {
  CUSTOM,
  LOCAL_URL,
  entryFor,
  hostOf,
  initialAddress,
  readConnection,
  serverChoices,
  writeConnection,
} from './connect'

/** A storage a test drives directly, optionally one that refuses everything. */
function fakeStorage(broken = false): Storage {
  const held = new Map<string, string>()
  return {
    get length() {
      return held.size
    },
    clear: () => held.clear(),
    key: (index) => [...held.keys()][index] ?? null,
    getItem: (key) => {
      if (broken) throw new Error('denied')
      return held.get(key) ?? null
    },
    setItem: (key, value) => {
      if (broken) throw new Error('denied')
      held.set(key, value)
    },
    removeItem: (key) => void held.delete(key),
  } as Storage
}

describe('the server list', () => {
  it('always offers this device and an address you type', () => {
    const choices = serverChoices()
    expect(choices.map((entry) => entry.id)).toContain('local')
    expect(choices.at(-1)?.id).toBe(CUSTOM)
    // Every entry a player can be pointed at carries where it is; the typed one has no region
    // because nobody has told this client where the address goes.
    for (const entry of choices) {
      if (entry.id !== CUSTOM) expect(entry.region).toBeTruthy()
    }
  })

  it('does not offer the same address twice', () => {
    // With no window and no build-time value, the resolved default *is* the local address, so
    // the configured entry would be a second row carrying one address.
    const urls = serverChoices().map((entry) => entry.url)
    expect(new Set(urls).size).toBe(urls.length)
  })

  it('reads an address that matches no entry as a custom one', () => {
    const choices = serverChoices()
    expect(entryFor(LOCAL_URL, choices).id).toBe('local')
    expect(entryFor('ws://elsewhere:9000', choices).id).toBe(CUSTOM)
  })

  it('names a row by the host it reaches', () => {
    expect(hostOf('wss://sage.example:443')).toBe('sage.example:443')
    expect(hostOf('ws://127.0.0.1:9000/socket')).toBe('127.0.0.1:9000')
    expect(hostOf('not an address')).toBe('not an address')
  })
})

describe('what this device remembers', () => {
  it('round-trips a name and a server', () => {
    const storage = fakeStorage()
    writeConnection(storage, { name: 'Ari', server: 'ws://elsewhere:9000' })
    expect(readConnection(storage)).toEqual({ name: 'Ari', server: 'ws://elsewhere:9000' })
  })

  it('reads no storage, broken storage, and nonsense as nothing remembered', () => {
    expect(readConnection(undefined)).toEqual({ name: '' })
    expect(readConnection(fakeStorage(true))).toEqual({ name: '' })
    const storage = fakeStorage()
    storage.setItem('sage.connect.v1', '{not json')
    expect(readConnection(storage)).toEqual({ name: '' })
    storage.setItem('sage.connect.v1', '"a string"')
    expect(readConnection(storage)).toEqual({ name: '' })
  })

  it('drops a stored name longer than the field allows', () => {
    const storage = fakeStorage()
    storage.setItem('sage.connect.v1', JSON.stringify({ name: 'x'.repeat(64) }))
    expect(readConnection(storage).name).toHaveLength(32)
  })

  it('carries on when the device refuses to remember', () => {
    expect(() => writeConnection(fakeStorage(true), { name: 'Ari' })).not.toThrow()
    expect(() => writeConnection(undefined, { name: 'Ari' })).not.toThrow()
  })
})

describe('the address to open with', () => {
  it('takes what this device chose over the resolved default', () => {
    const storage = fakeStorage()
    writeConnection(storage, { name: 'Ari', server: 'ws://elsewhere:9000' })
    expect(initialAddress(storage)).toBe('ws://elsewhere:9000')
  })

  it('falls back to the default when nothing was chosen', () => {
    expect(initialAddress(fakeStorage())).toBe(LOCAL_URL)
    expect(initialAddress(undefined)).toBe(LOCAL_URL)
  })
})
