/**
 * Device-local card-art blob cache (ADR 0024). Downloaded illustrations live in
 * IndexedDB so they survive reloads without re-downloading — they are cached
 * ONLY on the player's device and are never uploaded or redistributed. Where
 * IndexedDB is unavailable (private modes, jsdom tests), the cache degrades to
 * an in-memory map: art still works for the session, it just doesn't persist.
 *
 * This is presentation cache, never game state: deleting it (the settings
 * panel's "clear" action) changes nothing but pixels.
 */

/** One cached illustration: the image bytes plus provenance for attribution. */
export interface StoredArt {
  /** The image bytes as fetched. */
  blob: Blob;
  /** Which pipeline produced it (`bundled` or `scryfall`). */
  source: string;
  /** The real card name the art was resolved from (attribution/debugging). */
  sourceName: string;
  /** Epoch milliseconds when fetched. */
  fetchedAt: number;
}

/** The async blob-cache surface the art store depends on (injected in tests). */
export interface ArtBlobCache {
  /** The cached art for a functional id, if present. */
  get(functionalId: string): Promise<StoredArt | undefined>;
  /** Insert or replace the cached art for a functional id. */
  put(functionalId: string, art: StoredArt): Promise<void>;
  /** Remove everything (the settings panel's "clear downloaded art"). */
  clear(): Promise<void>;
  /** All cached functional ids. */
  keys(): Promise<string[]>;
}

/** Non-persistent fallback (and the test double): a Map with the cache surface. */
export class MemoryArtCache implements ArtBlobCache {
  private readonly entries = new Map<string, StoredArt>();

  get(functionalId: string): Promise<StoredArt | undefined> {
    return Promise.resolve(this.entries.get(functionalId));
  }

  put(functionalId: string, art: StoredArt): Promise<void> {
    this.entries.set(functionalId, art);
    return Promise.resolve();
  }

  clear(): Promise<void> {
    this.entries.clear();
    return Promise.resolve();
  }

  keys(): Promise<string[]> {
    return Promise.resolve([...this.entries.keys()]);
  }
}

/** IndexedDB database and object-store names for the persistent cache. */
const DB_NAME = 'rune-card-art';
const STORE_NAME = 'art';

/** Promisify one IDBRequest. */
function requestToPromise<T>(request: IDBRequest<T>): Promise<T> {
  return new Promise((resolve, reject) => {
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error ?? new Error('IndexedDB request failed'));
  });
}

/** IndexedDB-backed cache. Every operation opens lazily off one shared handle. */
class IdbArtCache implements ArtBlobCache {
  private db: Promise<IDBDatabase> | null = null;

  private open(): Promise<IDBDatabase> {
    this.db ??= new Promise((resolve, reject) => {
      const request = indexedDB.open(DB_NAME, 1);
      request.onupgradeneeded = () => {
        if (!request.result.objectStoreNames.contains(STORE_NAME)) {
          request.result.createObjectStore(STORE_NAME);
        }
      };
      request.onsuccess = () => resolve(request.result);
      request.onerror = () => reject(request.error ?? new Error('IndexedDB open failed'));
    });
    return this.db;
  }

  private async store(mode: IDBTransactionMode): Promise<IDBObjectStore> {
    const db = await this.open();
    return db.transaction(STORE_NAME, mode).objectStore(STORE_NAME);
  }

  async get(functionalId: string): Promise<StoredArt | undefined> {
    const store = await this.store('readonly');
    return (await requestToPromise(store.get(functionalId))) as StoredArt | undefined;
  }

  async put(functionalId: string, art: StoredArt): Promise<void> {
    const store = await this.store('readwrite');
    await requestToPromise(store.put(art, functionalId));
  }

  async clear(): Promise<void> {
    const store = await this.store('readwrite');
    await requestToPromise(store.clear());
  }

  async keys(): Promise<string[]> {
    const store = await this.store('readonly');
    return (await requestToPromise(store.getAllKeys())).map(String);
  }
}

/**
 * The persistent cache where IndexedDB exists, else the in-memory fallback.
 * Failures inside IndexedDB operations surface as rejected promises the art
 * store treats as cache misses — never as crashes.
 */
export function openArtCache(): ArtBlobCache {
  try {
    if (typeof indexedDB !== 'undefined') return new IdbArtCache();
  } catch {
    // Fall through to the memory cache.
  }
  return new MemoryArtCache();
}
