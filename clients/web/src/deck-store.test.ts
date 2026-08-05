/**
 * The decks this device kept.
 *
 * Half of these are the round trip; the other half are the ways a browser can let you down —
 * storage switched off, a key holding something else, a key holding nothing readable — every one
 * of which has to come back as "no decks kept" rather than as a screen that will not open.
 */
import { describe, expect, it } from 'vitest'

import { deleteDeck, draftOf, savedDecks, savedSize, saveDeck } from './deck-store'
import type { DeckDraft } from './deck'

const store = (start: Record<string, string> = {}): Storage => {
  const held = new Map(Object.entries(start))
  return {
    getItem: (key) => held.get(key) ?? null,
    setItem: (key, value) => void held.set(key, value),
    removeItem: (key) => void held.delete(key),
    clear: () => held.clear(),
    key: (at) => [...held.keys()][at] ?? null,
    get length() {
      return held.size
    },
  }
}

const DRAFT: DeckDraft = {
  entries: [{ identity: 'lightning_bolt', count: 4 }],
  side: [{ identity: 'tormods_crypt', count: 2 }],
  commander: 'krenko_mob_boss',
}

describe('decks kept on this device', () => {
  it('keeps the deck whole — the cards beside it and the commander included', () => {
    const storage = store()
    saveDeck(storage, 'Burn', DRAFT)

    const [kept] = savedDecks(storage)
    expect(kept?.name).toBe('Burn')
    expect(draftOf(kept!)).toEqual(DRAFT)
    expect(savedSize(kept!)).toBe(4)
  })

  it('replaces what a name already held rather than keeping two of it', () => {
    const storage = store()
    saveDeck(storage, 'Burn', DRAFT)
    const after = saveDeck(storage, 'Burn', { entries: [{ identity: 'goblin_guide', count: 1 }] })

    expect(after).toHaveLength(1)
    expect(after[0]?.entries).toEqual([{ identity: 'goblin_guide', count: 1 }])
  })

  it('keeps the list in name order, whatever order decks were saved in', () => {
    const storage = store()
    saveDeck(storage, 'Zoo', DRAFT)
    saveDeck(storage, 'Affinity', DRAFT)

    expect(savedDecks(storage).map((deck) => deck.name)).toEqual(['Affinity', 'Zoo'])
  })

  it('forgets one and leaves the others', () => {
    const storage = store()
    saveDeck(storage, 'Burn', DRAFT)
    saveDeck(storage, 'Zoo', DRAFT)

    expect(deleteDeck(storage, 'Burn').map((deck) => deck.name)).toEqual(['Zoo'])
    expect(savedDecks(storage).map((deck) => deck.name)).toEqual(['Zoo'])
  })

  it('reads a device with no storage at all as a device keeping no decks', () => {
    expect(savedDecks(undefined)).toEqual([])
    // And saving into one is not an error; it is a deck that will not be there next time.
    expect(() => saveDeck(undefined, 'Burn', DRAFT)).not.toThrow()
  })

  it('reads nonsense under the key as nothing kept', () => {
    expect(savedDecks(store({ 'sage.decks': 'not json' }))).toEqual([])
    expect(savedDecks(store({ 'sage.decks': '{"decks":1}' }))).toEqual([])
    // A list holding entries that are not decks keeps the ones that are.
    expect(savedDecks(store({ 'sage.decks': '[3, {"name":"Ok","entries":[]}]' }))).toEqual([
      { name: 'Ok', entries: [] },
    ])
  })
})
