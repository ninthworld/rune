/**
 * The decks this device has kept.
 *
 * Device storage in the manner of `connect.ts` and ADR 0012's art preference: written to this
 * browser and nowhere else, never sent anywhere, and **absent or unreadable is always a working
 * answer** — a player with storage switched off builds decks and loses them on reload, which is
 * a worse day than a player with it on and a better one than a screen that will not open.
 *
 * A saved deck is the draft, by name. It holds the sideboard and the commander too, because they
 * are part of the deck the player built even where `submit_deck` carries neither.
 */
import type { DeckDraft, DeckEntry } from './deck'

const KEY = 'sage.decks'

/** One deck kept on this device. */
export interface SavedDeck {
  name: string
  entries: readonly DeckEntry[]
  side?: readonly DeckEntry[]
  commander?: string
}

const entries = (value: unknown): DeckEntry[] =>
  Array.isArray(value)
    ? value.flatMap((entry) =>
        entry !== null &&
        typeof entry === 'object' &&
        typeof (entry as DeckEntry).identity === 'string' &&
        typeof (entry as DeckEntry).count === 'number'
          ? [{ identity: (entry as DeckEntry).identity, count: (entry as DeckEntry).count }]
          : [],
      )
    : []

/** Everything kept here, by name. Anything unreadable is treated as nothing kept. */
export function savedDecks(storage: Storage | undefined): readonly SavedDeck[] {
  let raw: string | null
  try {
    raw = storage?.getItem(KEY) ?? null
  } catch {
    return []
  }
  if (raw === null) return []

  try {
    const parsed: unknown = JSON.parse(raw)
    if (!Array.isArray(parsed)) return []
    return parsed.flatMap((deck) => {
      if (deck === null || typeof deck !== 'object') return []
      const held = deck as Partial<SavedDeck>
      if (typeof held.name !== 'string' || held.name === '') return []
      const side = entries(held.side)
      return [
        {
          name: held.name,
          entries: entries(held.entries),
          ...(side.length > 0 ? { side } : {}),
          ...(typeof held.commander === 'string' ? { commander: held.commander } : {}),
        },
      ]
    })
  } catch {
    return []
  }
}

/** Writing is best-effort: a full or blocked store loses the save, never the deck on screen. */
function write(storage: Storage | undefined, decks: readonly SavedDeck[]): void {
  try {
    storage?.setItem(KEY, JSON.stringify(decks))
  } catch {
    /* a device that will not keep decks is a device that does not keep them */
  }
}

/** Keep a deck under a name, replacing whatever that name held. */
export function saveDeck(
  storage: Storage | undefined,
  name: string,
  draft: DeckDraft,
): readonly SavedDeck[] {
  const saved: SavedDeck = {
    name,
    entries: draft.entries,
    ...(draft.side !== undefined && draft.side.length > 0 ? { side: draft.side } : {}),
    ...(draft.commander === undefined ? {} : { commander: draft.commander }),
  }
  const next = [...savedDecks(storage).filter((deck) => deck.name !== name), saved].sort((a, b) =>
    a.name.localeCompare(b.name),
  )
  write(storage, next)
  return next
}

/** Forget one. There is no undo, which is why the surface asks first. */
export function deleteDeck(storage: Storage | undefined, name: string): readonly SavedDeck[] {
  const next = savedDecks(storage).filter((deck) => deck.name !== name)
  write(storage, next)
  return next
}

/** A kept deck as a draft the builder can hold. */
export const draftOf = (deck: SavedDeck): DeckDraft => ({
  entries: deck.entries,
  ...(deck.side === undefined ? {} : { side: deck.side }),
  ...(deck.commander === undefined ? {} : { commander: deck.commander }),
})

/** How many cards a kept deck holds, sideboard apart. */
export const savedSize = (deck: SavedDeck): number =>
  deck.entries.reduce((total, entry) => total + entry.count, 0)
