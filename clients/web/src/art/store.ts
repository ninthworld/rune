/**
 * The art registry: a texture per card identity, resolved in the background and never waited on.
 *
 * ADR 0012's load-bearing sentence is **art is cache, never state**, and this module is where
 * that is either true or not. Everything it holds can be thrown away at any moment and the game
 * is unaffected: the UI is reconstructable from one `GameView` with this store empty, every card
 * falls back to its procedural face per card, and no render ever blocks on a request. An art
 * source that is slow, rate-limited, unreachable, or wrong about a card cannot delay a click.
 *
 * Three rules shape the implementation:
 *
 * 1. **One request per card, ever.** A card resolves once and is remembered — as an image or as a
 *    miss. Sixty cards on a board asking sixty times a second is the failure mode a registry
 *    exists to prevent, so `request` is idempotent and knows what it has already asked.
 * 2. **One request at a time, spaced.** The queue drains serially with the gap the source asked
 *    for. A client that opened forty connections to a free service would be the reason that
 *    service stops being free.
 * 3. **Failure and absence are different.** A card the source does not have is remembered as a
 *    miss and never asked again. A request that *failed* is forgotten, so it can be retried the
 *    next time something needs it — a dropped network must not permanently blank a card.
 *
 * Nothing here is React. The clock, the network, and the storage are all injected, which is what
 * lets the tests cover the queue, the cache, and the failure paths without a browser or a socket.
 */
import type { ArtImages, ArtSource } from './source'

/** What the store knows about one card identity. */
export type ArtEntry =
  | { status: 'loading' }
  | { status: 'ready'; images: ArtImages }
  /** The source answered, and has nothing for this card. Not an error, and not retried. */
  | { status: 'missing' }

export interface ArtStoreOptions {
  /** Absent means nothing resolves: the procedural default, and no network at all. */
  source?: ArtSource
  fetch?: typeof globalThis.fetch
  /** Where resolved URLs are cached, device-local. Absent means this session only. */
  storage?: Storage
  /** Injected so the queue's spacing is a test's to control rather than a test's to wait for. */
  delay?(ms: number): Promise<void>
}

const CACHE_KEY = 'sage.art.cache.v1'

const sleep = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms))

export class ArtStore {
  private entries = new Map<string, ArtEntry>()
  private queue: { key: string; name: string }[] = []
  private listeners = new Set<() => void>()
  private draining = false
  private source?: ArtSource
  private readonly fetch: typeof globalThis.fetch
  private readonly storage?: Storage
  private readonly delay: (ms: number) => Promise<void>

  constructor(options: ArtStoreOptions = {}) {
    this.source = options.source
    this.fetch = options.fetch ?? globalThis.fetch?.bind(globalThis)
    this.storage = options.storage
    this.delay = options.delay ?? sleep
    this.entries = this.readCache()
  }

  /** What is known about this card right now. `undefined` means nothing has been asked. */
  get(key: string): ArtEntry | undefined {
    return this.entries.get(key)
  }

  /**
   * Ask for this card, if it has not been asked for already.
   *
   * Safe to call on every render of every card on the board: an identity already known, already
   * queued, or already in flight is a no-op, and with no source configured nothing happens at
   * all — which is what makes the procedural default cost exactly nothing.
   */
  request(key: string, name: string): void {
    if (!this.source || !this.fetch) return
    if (this.entries.has(key)) return
    if (this.queue.some((queued) => queued.key === key)) return

    this.queue.push({ key, name })
    void this.drain()
  }

  /**
   * Point the store at a different source, or at none.
   *
   * Everything resolved is dropped, because an entry is only meaningful as *that source's*
   * answer — keeping it would show one source's art under another's name. The queue goes with it:
   * a player who has just turned the source off has said they do not want those requests made.
   */
  setSource(source: ArtSource | undefined): void {
    if (this.source?.id === source?.id) return
    this.source = source
    this.queue = []
    this.entries = source ? this.readCache() : new Map()
    this.announce()
  }

  /**
   * How many cards this device has a settled answer for.
   *
   * A number rather than a list, because what a player is deciding is whether there is anything
   * to clear — and it is one number so a subscriber can compare it without allocating.
   */
  cached(): number {
    let settled = 0
    for (const entry of this.entries.values()) if (entry.status !== 'loading') settled += 1
    return settled
  }

  /**
   * How many cards are still queued or in flight.
   *
   * What the bulk download in settings (§9.6) reports progress against, and the only thing that
   * distinguishes "downloading" from "done" — the queue drains serially and spaced, so a player
   * who asked for a whole catalog is waiting on a number that only falls.
   */
  waiting(): number {
    return this.queue.length + (this.draining ? 1 : 0)
  }

  /**
   * Stop asking for anything else, keeping everything already resolved.
   *
   * The way out of a bulk download. The request in flight is not aborted — it is one card, and a
   * cancelled fetch would be remembered as a failure and re-asked later for no gain.
   */
  stop(): void {
    if (this.queue.length === 0) return
    this.queue = []
    this.announce()
  }

  /** Forget every resolved image, on this device. The next look re-asks. */
  clear(): void {
    this.entries = new Map()
    this.queue = []
    try {
      this.storage?.removeItem(CACHE_KEY)
    } catch {
      // Nothing to do: the in-memory registry is cleared either way.
    }
    this.announce()
  }

  subscribe(listener: () => void): () => void {
    this.listeners.add(listener)
    return () => this.listeners.delete(listener)
  }

  private announce(): void {
    for (const listener of this.listeners) listener()
  }

  private async drain(): Promise<void> {
    if (this.draining) return
    this.draining = true
    try {
      for (;;) {
        const next = this.queue.shift()
        if (!next) return
        const source = this.source
        if (!source || !this.fetch) return

        this.entries.set(next.key, { status: 'loading' })
        this.announce()

        try {
          const images = await source.resolve(next, this.fetch)
          this.entries.set(next.key, images ? { status: 'ready', images } : { status: 'missing' })
          this.writeCache()
        } catch {
          // Forgotten rather than remembered as a miss: a network that was down for one card
          // must not blank that card for the rest of the session.
          this.entries.delete(next.key)
        }

        this.announce()
        // After the request, not before: the gap the source asked for is a gap between calls,
        // and paying it up front would delay the first card of a fresh board for nothing.
        if (this.queue.length > 0) await this.delay(source.minimumIntervalMs)
      }
    } finally {
      this.draining = false
    }
  }

  /**
   * The device-local cache.
   *
   * Resolved URLs, not image bytes: the browser's own HTTP cache already holds the pixels, and
   * duplicating megabytes of them into a storage quota to save one redirect is the wrong trade.
   * What this saves is the *lookup* — the request to the source — which is the part that is rate
   * limited and the part that is a burden on somebody else's service.
   *
   * Nothing here ever leaves the device. It is not sent to the server, not shared with another
   * client in the same game, and not part of any message.
   */
  private readCache(): Map<string, ArtEntry> {
    try {
      const raw: unknown = JSON.parse(this.storage?.getItem(CACHE_KEY) ?? 'null')
      if (typeof raw !== 'object' || raw === null) return new Map()
      const entries = Object.entries(raw as Record<string, ArtEntry>).filter(
        // A half-written entry, or one from a build whose shape differed. Dropping it costs one
        // lookup; trusting it would draw a broken image.
        ([, entry]) => entry?.status === 'ready' || entry?.status === 'missing',
      )
      return new Map(entries)
    } catch {
      return new Map()
    }
  }

  private writeCache(): void {
    try {
      const settled = [...this.entries].filter(([, entry]) => entry.status !== 'loading')
      this.storage?.setItem(CACHE_KEY, JSON.stringify(Object.fromEntries(settled)))
    } catch {
      // A full or disabled storage costs a lookup next session and nothing else.
    }
  }
}
