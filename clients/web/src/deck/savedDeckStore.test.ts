import { afterEach, describe, expect, it } from 'vitest';
import {
  MemorySavedDeckDb,
  cardsToCounts,
  configureSavedDeckStore,
  countsToCards,
  deleteSavedDeck,
  listSavedDecks,
  loadSavedDeck,
  resetSavedDeckStore,
  saveDeck,
  savedDeckExists,
  type SavedDeckDb,
} from './savedDeckStore';

afterEach(() => {
  resetSavedDeckStore();
});

/** A backing store whose every operation rejects — the "storage unavailable" case. */
function failingDb(): SavedDeckDb {
  const fail = (): Promise<never> => Promise.reject(new Error('storage unavailable'));
  return { getAll: fail, get: fail, put: fail, delete: fail };
}

describe('savedDeckStore (ADR 0027)', () => {
  it('collapses builder counts into stable, positive card rows', () => {
    const cards = countsToCards({ shock: 4, serra_angel: 0, forest: 17, plains: -2 });
    // Zero/negative dropped; ids sorted for a deterministic export/round-trip.
    expect(cards).toEqual([
      { functional_id: 'forest', count: 17 },
      { functional_id: 'shock', count: 4 },
    ]);
  });

  it('expands card rows back into builder counts', () => {
    expect(cardsToCounts([{ functional_id: 'shock', count: 4 }])).toEqual({ shock: 4 });
  });

  it('saves a deck and lists it back', async () => {
    configureSavedDeckStore({ db: new MemorySavedDeckDb(), now: () => 1000 });
    await saveDeck({ name: 'Burn', cards: countsToCards({ shock: 4 }) });
    const list = await listSavedDecks();
    expect(list).toHaveLength(1);
    expect(list[0]).toEqual({
      name: 'Burn',
      cards: [{ functional_id: 'shock', count: 4 }],
      updatedAt: 1000,
    });
  });

  it('keeps a saved deck across a new session on the same device', async () => {
    // The device's IndexedDB survives a reload: the store singleton resets but the
    // backing db instance is the same device storage.
    const db = new MemorySavedDeckDb();
    configureSavedDeckStore({ db, now: () => 1 });
    await saveDeck({ name: 'Angels', cards: countsToCards({ serra_angel: 2 }) });

    // Simulate a fresh session: drop the singleton, re-open against the same device.
    resetSavedDeckStore();
    configureSavedDeckStore({ db, now: () => 2 });
    const list = await listSavedDecks();
    expect(list.map((d) => d.name)).toEqual(['Angels']);
  });

  it('overwrites a deck saved under the same name (explicit-intent gate is the UI)', async () => {
    configureSavedDeckStore({ db: new MemorySavedDeckDb(), now: () => 5 });
    await saveDeck({ name: 'Deck', cards: countsToCards({ shock: 1 }) });
    expect(await savedDeckExists('Deck')).toBe(true);
    await saveDeck({ name: 'Deck', cards: countsToCards({ shock: 4 }) });
    const list = await listSavedDecks();
    expect(list).toHaveLength(1);
    expect(list[0].cards).toEqual([{ functional_id: 'shock', count: 4 }]);
  });

  it('trims the name and rejects an empty one', async () => {
    configureSavedDeckStore({ db: new MemorySavedDeckDb(), now: () => 5 });
    await saveDeck({ name: '  Padded  ', cards: countsToCards({ shock: 1 }) });
    expect(await savedDeckExists('Padded')).toBe(true);
    await expect(saveDeck({ name: '   ', cards: [] })).rejects.toThrow();
  });

  it('loads and deletes a saved deck by name', async () => {
    configureSavedDeckStore({ db: new MemorySavedDeckDb(), now: () => 5 });
    await saveDeck({ name: 'Temp', cards: countsToCards({ forest: 10 }) });
    expect((await loadSavedDeck('Temp'))?.cards).toEqual([{ functional_id: 'forest', count: 10 }]);
    await deleteSavedDeck('Temp');
    expect(await loadSavedDeck('Temp')).toBeUndefined();
    expect(await listSavedDecks()).toHaveLength(0);
  });

  it('lists decks sorted case-insensitively by name', async () => {
    configureSavedDeckStore({ db: new MemorySavedDeckDb(), now: () => 5 });
    await saveDeck({ name: 'zebra', cards: countsToCards({ shock: 1 }) });
    await saveDeck({ name: 'Apple', cards: countsToCards({ shock: 1 }) });
    expect((await listSavedDecks()).map((d) => d.name)).toEqual(['Apple', 'zebra']);
  });

  it('surfaces storage failures as rejections the caller degrades on', async () => {
    configureSavedDeckStore({ db: failingDb() });
    await expect(listSavedDecks()).rejects.toThrow('storage unavailable');
    await expect(saveDeck({ name: 'X', cards: countsToCards({ shock: 1 }) })).rejects.toThrow();
  });
});
