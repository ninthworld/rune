/**
 * The art registry, entirely offline.
 *
 * Every test here injects its own fetch. Nothing in this suite touches the network, which is the
 * standing requirement for the pipeline of ADR 0012: a third party's availability is outside this
 * project's control and must never be able to fail a build.
 */
import { describe, expect, it, vi } from 'vitest'

import { ArtStore } from './store'
import { SCRYFALL, type ArtImages, type ArtSource } from './source'

const IMAGES: ArtImages = {
  window: 'https://example.test/art.jpg',
  full: 'https://example.test/card.jpg',
}

/** A source that answers from a table, and counts what it was asked. */
function fakeSource(
  answers: Record<string, ArtImages | undefined | Error> = {},
): ArtSource & { asked: string[] } {
  const asked: string[] = []
  return {
    id: 'scryfall',
    label: 'Fake',
    home: 'https://example.test',
    minimumIntervalMs: 100,
    asked,
    async resolve({ name }) {
      asked.push(name)
      const answer = answers[name]
      if (answer instanceof Error) throw answer
      return answer
    },
  }
}

/** A storage that behaves, so the cache path is exercised without a browser. */
function memoryStorage(): Storage {
  const map = new Map<string, string>()
  return {
    get length() {
      return map.size
    },
    clear: () => map.clear(),
    getItem: (key) => map.get(key) ?? null,
    key: (index) => [...map.keys()][index] ?? null,
    removeItem: (key) => void map.delete(key),
    setItem: (key, value) => void map.set(key, value),
  }
}

const settled = () => new Promise((resolve) => setTimeout(resolve, 0))

const storeWith = (source: ArtSource | undefined, storage?: Storage) =>
  new ArtStore({
    source,
    storage,
    fetch: (async () => new Response()) as typeof fetch,
    delay: async () => {},
  })

describe('resolving art', () => {
  it('resolves a card once and remembers it', async () => {
    const source = fakeSource({ 'Lightning Bolt': IMAGES })
    const store = storeWith(source)

    store.request('lightning_bolt', 'Lightning Bolt')
    await settled()

    expect(store.get('lightning_bolt')).toEqual({ status: 'ready', images: IMAGES })
    // Safe to call on every render of every card on the board: a card already known is a no-op.
    store.request('lightning_bolt', 'Lightning Bolt')
    store.request('lightning_bolt', 'Lightning Bolt')
    await settled()
    expect(source.asked).toEqual(['Lightning Bolt'])
  })

  it('remembers a card the source does not have, and stops asking', async () => {
    // The ordinary answer for a token, or for a card this project made up. Not an error.
    const source = fakeSource({ 'Made Up': undefined })
    const store = storeWith(source)

    store.request('made_up', 'Made Up')
    await settled()
    expect(store.get('made_up')).toEqual({ status: 'missing' })

    store.request('made_up', 'Made Up')
    await settled()
    expect(source.asked).toEqual(['Made Up'])
  })

  it('forgets a request that failed, so it can be retried', async () => {
    // A network that was down for one card must not blank that card for the rest of the session.
    const source = fakeSource({ Bolt: new Error('offline') })
    const store = storeWith(source)

    store.request('bolt', 'Bolt')
    await settled()

    expect(store.get('bolt')).toBeUndefined()
  })

  it('asks for one card at a time', async () => {
    const source = fakeSource({ A: IMAGES, B: IMAGES, C: IMAGES })
    const inFlight: number[] = []
    let open = 0
    const counted: ArtSource = {
      ...source,
      async resolve(request, fetch) {
        open += 1
        inFlight.push(open)
        try {
          return await source.resolve(request, fetch)
        } finally {
          open -= 1
        }
      },
    }
    const store = storeWith(counted)

    store.request('a', 'A')
    store.request('b', 'B')
    store.request('c', 'C')
    await settled()

    expect(source.asked).toEqual(['A', 'B', 'C'])
    expect(Math.max(...inFlight)).toBe(1)
  })

  it('waits the interval the source asked for between requests', async () => {
    const source = fakeSource({ A: IMAGES, B: IMAGES })
    const delay = vi.fn(async () => {})
    const store = new ArtStore({
      source,
      delay,
      fetch: (async () => new Response()) as typeof fetch,
    })

    store.request('a', 'A')
    store.request('b', 'B')
    await settled()

    // Between the two, and not before the first: a gap paid up front would delay the first card
    // of a fresh board for nothing.
    expect(delay).toHaveBeenCalledTimes(1)
    expect(delay).toHaveBeenCalledWith(100)
  })

  it('does nothing at all with no source', async () => {
    const store = storeWith(undefined)
    store.request('lightning_bolt', 'Lightning Bolt')
    await settled()
    expect(store.get('lightning_bolt')).toBeUndefined()
  })
})

describe('the device-local cache', () => {
  it('re-reads a resolved card without asking the source again', async () => {
    const storage = memoryStorage()
    const first = fakeSource({ Bolt: IMAGES })
    const store = storeWith(first, storage)
    store.request('bolt', 'Bolt')
    await settled()

    const second = fakeSource({ Bolt: IMAGES })
    const reopened = storeWith(second, storage)
    expect(reopened.get('bolt')).toEqual({ status: 'ready', images: IMAGES })
    reopened.request('bolt', 'Bolt')
    await settled()
    expect(second.asked).toEqual([])
  })

  it('never caches a request still in flight', async () => {
    const storage = memoryStorage()
    const store = storeWith(fakeSource({ Bolt: IMAGES }), storage)
    store.request('bolt', 'Bolt')
    await settled()

    const raw = JSON.parse(storage.getItem('sage.art.cache.v1') ?? '{}') as Record<
      string,
      { status: string }
    >
    expect(Object.values(raw).some((entry) => entry.status === 'loading')).toBe(false)
  })

  it('survives a cache a different build wrote', async () => {
    // Art is cache and never state, so anything unreadable costs one lookup and nothing else.
    const storage = memoryStorage()
    storage.setItem('sage.art.cache.v1', '{"bolt":{"status":"whatever"}}')
    const store = storeWith(fakeSource({ Bolt: IMAGES }), storage)
    expect(store.get('bolt')).toBeUndefined()
  })

  it('clearing forgets everything, on the device too', async () => {
    const storage = memoryStorage()
    const store = storeWith(fakeSource({ Bolt: IMAGES }), storage)
    store.request('bolt', 'Bolt')
    await settled()

    store.clear()
    expect(store.get('bolt')).toBeUndefined()
    expect(storage.getItem('sage.art.cache.v1')).toBeNull()
  })

  it('drops everything resolved when the source changes', async () => {
    // An entry is only meaningful as that source's answer; keeping it would show one source's
    // art under another's name.
    const store = storeWith(fakeSource({ Bolt: IMAGES }))
    store.request('bolt', 'Bolt')
    await settled()

    store.setSource(undefined)
    expect(store.get('bolt')).toBeUndefined()
  })
})

describe('the Scryfall source', () => {
  it('asks by exact name and nothing else', async () => {
    // No game data leaves this client beyond the name being resolved.
    const fetch = vi.fn(async () =>
      Response.json({ image_uris: { art_crop: 'a', normal: 'n' } }),
    ) as unknown as typeof globalThis.fetch

    const images = await SCRYFALL.resolve({ key: 'lightning_bolt', name: 'Lightning Bolt' }, fetch)

    expect(images).toEqual({ window: 'a', full: 'n' })
    expect(vi.mocked(fetch).mock.calls[0]?.[0]).toBe(
      'https://api.scryfall.com/cards/named?exact=Lightning%20Bolt',
    )
  })

  it('reads a double-faced card from its front face', async () => {
    const fetch = vi.fn(async () =>
      Response.json({ card_faces: [{ image_uris: { art_crop: 'front' } }] }),
    ) as unknown as typeof globalThis.fetch

    expect(await SCRYFALL.resolve({ key: 'x', name: 'X' }, fetch)).toEqual({
      window: 'front',
      full: undefined,
    })
  })

  it('reads a 404 as a card the source does not have', async () => {
    const fetch = vi.fn(async () => new Response('', { status: 404 })) as typeof globalThis.fetch
    expect(await SCRYFALL.resolve({ key: 'x', name: 'X' }, fetch)).toBeUndefined()
  })

  it('throws on any other failure, so the card is retried rather than blanked', async () => {
    const fetch = vi.fn(async () => new Response('', { status: 503 })) as typeof globalThis.fetch
    await expect(SCRYFALL.resolve({ key: 'x', name: 'X' }, fetch)).rejects.toThrow()
  })
})
