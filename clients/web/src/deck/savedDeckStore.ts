/**
 * Device-local saved decks (issue #369, ADR 0027).
 *
 * A built deck survives to the next session by living on the player's own device:
 * saved decks are persisted in IndexedDB — alongside the ADR 0024 art cache — keyed
 * by a player-chosen name. This is an explicit, deliberate carve-out from the
 * `localStorage`-of-game-state rule (see `clients/web/AGENTS.md`): a saved deck is
 * pre-game, player-authored *input*, never a rendered view or load-bearing across
 * protocol messages. The builder and every view must still reconstruct fully with
 * this store empty.
 *
 * The server stays stateless: NO protocol change, NO server storage, NO durable
 * identity/token. The only cross-device / sharing mechanism is the portable
 * export/import document (`deckDocument.ts`); this module ships no sync.
 *
 * Saving never implies legality (ADR 0027): a saved deck is a plain (name, cards)
 * record. It is validated only at submission time, by the room format, through the
 * UNCHANGED `submit_deck` gate. A deck saved under one format may be rejected by
 * another without corrupting the saved copy.
 *
 * Where IndexedDB is unavailable (private modes, disabled storage, jsdom tests),
 * the store degrades to an in-memory map: saving still works for the session, it
 * just does not persist. Operational failures (quota, blocked transactions) surface
 * as rejected promises the caller degrades on — never as a broken screen. Mirrors
 * the art cache's idioms and graceful-degradation posture exactly.
 *
 * The module is a singleton with an injectable backing store (`configureSavedDeckStore`)
 * and a full reset (`resetSavedDeckStore`) so tests run deterministic and offline.
 */

/** One card row of a deck: an opaque `functional_id` and its copy count. */
export interface DeckCard {
  /** The card's authored `functional_id` — the only cross-build-stable identity. */
  readonly functional_id: string;
  /** How many copies of this card the deck runs (always ≥ 1 when stored). */
  readonly count: number;
}

/** A named deck's contents — the portable payload (no storage metadata). */
export interface DeckContents {
  /** The player-chosen name; the storage key and the export document's title. */
  readonly name: string;
  /** The deck's card rows (functional_id + count), in stable order. */
  readonly cards: readonly DeckCard[];
}

/** A stored deck: its contents plus the device-local save timestamp. */
export interface SavedDeck extends DeckContents {
  /** Epoch milliseconds of the last save (device-local metadata; not exported). */
  readonly updatedAt: number;
}

/**
 * The async persistence surface the store depends on (injected in tests). Keyed by
 * the deck's `name`; `put` inserts or overwrites, so overwrite protection is the
 * caller's responsibility (explicit-intent rule).
 */
export interface SavedDeckDb {
  /** Every saved deck (unordered). */
  getAll(): Promise<SavedDeck[]>;
  /** The saved deck for a name, or `undefined` if none. */
  get(name: string): Promise<SavedDeck | undefined>;
  /** Insert or replace the saved deck under its name. */
  put(deck: SavedDeck): Promise<void>;
  /** Remove the saved deck for a name (a no-op if absent). */
  delete(name: string): Promise<void>;
}

/** Non-persistent fallback (and the test double): a Map with the db surface. */
export class MemorySavedDeckDb implements SavedDeckDb {
  private readonly entries = new Map<string, SavedDeck>();

  getAll(): Promise<SavedDeck[]> {
    return Promise.resolve([...this.entries.values()]);
  }

  get(name: string): Promise<SavedDeck | undefined> {
    return Promise.resolve(this.entries.get(name));
  }

  put(deck: SavedDeck): Promise<void> {
    this.entries.set(deck.name, deck);
    return Promise.resolve();
  }

  delete(name: string): Promise<void> {
    this.entries.delete(name);
    return Promise.resolve();
  }
}

/** IndexedDB database and object-store names for the persistent saved decks. */
const DB_NAME = 'rune-saved-decks';
const STORE_NAME = 'decks';

/** Promisify one IDBRequest. */
function requestToPromise<T>(request: IDBRequest<T>): Promise<T> {
  return new Promise((resolve, reject) => {
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error ?? new Error('IndexedDB request failed'));
  });
}

/** IndexedDB-backed store. Every operation opens lazily off one shared handle. */
class IdbSavedDeckDb implements SavedDeckDb {
  private db: Promise<IDBDatabase> | null = null;

  private open(): Promise<IDBDatabase> {
    this.db ??= new Promise((resolve, reject) => {
      const request = indexedDB.open(DB_NAME, 1);
      request.onupgradeneeded = () => {
        if (!request.result.objectStoreNames.contains(STORE_NAME)) {
          // Keyed by the deck's `name` (in-line keyPath), so re-saving a name overwrites.
          request.result.createObjectStore(STORE_NAME, { keyPath: 'name' });
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

  async getAll(): Promise<SavedDeck[]> {
    const store = await this.store('readonly');
    return (await requestToPromise(store.getAll())) as SavedDeck[];
  }

  async get(name: string): Promise<SavedDeck | undefined> {
    const store = await this.store('readonly');
    return (await requestToPromise(store.get(name))) as SavedDeck | undefined;
  }

  async put(deck: SavedDeck): Promise<void> {
    const store = await this.store('readwrite');
    await requestToPromise(store.put(deck));
  }

  async delete(name: string): Promise<void> {
    const store = await this.store('readwrite');
    await requestToPromise(store.delete(name));
  }
}

/**
 * The persistent store where IndexedDB exists, else the in-memory fallback.
 * Failures inside IndexedDB operations surface as rejected promises the caller
 * degrades on — never as crashes.
 */
export function openSavedDeckDb(): SavedDeckDb {
  try {
    if (typeof indexedDB !== 'undefined') return new IdbSavedDeckDb();
  } catch {
    // Fall through to the memory store.
  }
  return new MemorySavedDeckDb();
}

/** Injectable effects, defaulted for the browser and overridable in tests. */
export interface SavedDeckStoreDeps {
  /** Device-local persistence for saved decks. */
  db: SavedDeckDb;
  /** Wall clock for save timestamps; tests pin it. */
  now: () => number;
}

/** Browser defaults: IndexedDB persistence and the real clock. */
function defaultDeps(): SavedDeckStoreDeps {
  return { db: openSavedDeckDb(), now: () => Date.now() };
}

let deps: SavedDeckStoreDeps | null = null;

/** The live effects, constructed on first access. */
function store(): SavedDeckStoreDeps {
  deps ??= defaultDeps();
  return deps;
}

/** Replace effect implementations (tests). */
export function configureSavedDeckStore(overrides: Partial<SavedDeckStoreDeps>): void {
  deps = { ...defaultDeps(), ...overrides };
}

/** Drop the configured effects — a fresh store on next access (tests). */
export function resetSavedDeckStore(): void {
  deps = null;
}

/** Trim a player-chosen deck name; the empty string is not a valid name. */
export function normalizeDeckName(name: string): string {
  return name.trim();
}

/**
 * Collapse builder `identity → copies` counts into stable, positive card rows.
 * Zero/negative counts are dropped and ids are sorted so a save is deterministic
 * (stable export, stable round-trip). Pure data assembly — no legality.
 */
export function countsToCards(counts: Readonly<Record<string, number>>): DeckCard[] {
  return Object.entries(counts)
    .filter(([, count]) => count > 0)
    .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
    .map(([functional_id, count]) => ({ functional_id, count }));
}

/** Expand card rows back into builder `identity → copies` counts. */
export function cardsToCounts(cards: readonly DeckCard[]): Record<string, number> {
  const counts: Record<string, number> = {};
  for (const card of cards) {
    if (card.count > 0) counts[card.functional_id] = (counts[card.functional_id] ?? 0) + card.count;
  }
  return counts;
}

/** Every saved deck, sorted by name (case-insensitive) for a stable list. */
export async function listSavedDecks(): Promise<SavedDeck[]> {
  const decks = await store().db.getAll();
  return decks.sort((a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: 'base' }));
}

/** Load one saved deck by name, or `undefined` if none. */
export function loadSavedDeck(name: string): Promise<SavedDeck | undefined> {
  return store().db.get(normalizeDeckName(name));
}

/** Whether a saved deck already exists under a name (overwrite-intent check). */
export async function savedDeckExists(name: string): Promise<boolean> {
  return (await loadSavedDeck(name)) !== undefined;
}

/**
 * Save (insert or overwrite) a named deck from its contents, stamping the save
 * time. Overwriting an existing name is intentional here — the caller enforces
 * explicit intent (the confirm affordance) before calling. Throws on an empty
 * name or a storage failure, so the caller can degrade.
 */
export async function saveDeck(contents: DeckContents): Promise<SavedDeck> {
  const name = normalizeDeckName(contents.name);
  if (name === '') throw new Error('A saved deck needs a name.');
  const deck: SavedDeck = { name, cards: [...contents.cards], updatedAt: store().now() };
  await store().db.put(deck);
  return deck;
}

/** Delete a saved deck by name (explicit intent enforced by the caller). */
export function deleteSavedDeck(name: string): Promise<void> {
  return store().db.delete(normalizeDeckName(name));
}
