/**
 * What the room's deck dropdown offers, and where it comes from (issue #546).
 *
 * #506 rendered the whole starter library as a tile grid in a right-hand column,
 * which is the "deck library as a permanent panel" the baseline replaces. The
 * library itself is not lost — it is contextual: the dropdown lists the bundled
 * starters plus whatever the player has saved on this device (ADR 0027), and the
 * quiet Edit/Import shortcuts open the builder, which owns that interface (#508
 * owns its visual system, deliberately out of scope here).
 *
 * No card logic. The bundled decklists are static names/ids (`decklists.ts`),
 * saved decks are device-local player input, and **every submission is validated
 * authoritatively server-side behind the unchanged `submit_deck` gate**. Choosing
 * a deck here claims nothing about its legality.
 */
import { useEffect, useState } from 'react';
import { STARTER_DECKLISTS, decklistCards } from '../decklists';
import type { CardIdentity } from '../protocol';
import { listSavedDecks, type SavedDeck } from '../deck/savedDeckStore';
import { commanderName } from './deckPresentation';

/** A deck the player can pick, from either source, flattened for the dropdown. */
export interface DeckChoiceOption {
  /** Stable option id: a starter's id, or `saved:<name>` for a device-local one. */
  readonly id: string;
  /** The name drawn in the dropdown and on the seat plaque. */
  readonly name: string;
  /** The flat card-identity list a `submit_deck` carries. */
  readonly cards: CardIdentity[];
  /** The deck's designated commander, when it has one. */
  readonly commander?: CardIdentity;
  /**
   * That commander's display name, resolved from the deck's own rows where they
   * are known. A saved deck carries identities only, so this falls back to the
   * opaque identity — which the client still never parses for meaning.
   */
  readonly commanderLabel?: string;
}

/** The `saved:` prefix that keeps a saved deck's id from colliding with a starter's. */
const SAVED_PREFIX = 'saved:';

/** Flatten a saved deck's `(functional_id, count)` rows into the wire's flat list. */
function savedCards(deck: SavedDeck): CardIdentity[] {
  return deck.cards.flatMap((row) => Array.from({ length: row.count }, () => row.functional_id));
}

/** Every deck offered, bundled starters first, then this device's saved decks. */
export function deckOptions(saved: readonly SavedDeck[]): DeckChoiceOption[] {
  const starters = STARTER_DECKLISTS.map((deck) => ({
    id: deck.id,
    name: deck.name,
    cards: decklistCards(deck),
    commander: deck.commander,
    commanderLabel: commanderName(deck),
  }));
  return [
    ...starters,
    ...saved.map((deck) => ({
      id: `${SAVED_PREFIX}${deck.name}`,
      name: deck.name,
      cards: savedCards(deck),
      commander: deck.commander,
      commanderLabel: deck.commander,
    })),
  ];
}

/** Resolve a picked option id, falling back to the first starter. */
export function deckOptionById(
  options: readonly DeckChoiceOption[],
  id: string,
): DeckChoiceOption | undefined {
  return options.find((option) => option.id === id) ?? options[0];
}

/**
 * The counts the builder opens on: whatever the picked option actually holds,
 * folded back from its flat identity list.
 *
 * Deliberately derived from `cards` rather than from a bundled `Decklist`, so a
 * *saved* deck opens as itself instead of as the first starter. Counting
 * identities is not card logic — it is the inverse of the flattening
 * `submit_deck` does, and the server remains the only authority on legality.
 */
export function optionCounts(option: DeckChoiceOption | undefined): Record<CardIdentity, number> {
  const counts: Record<CardIdentity, number> = {};
  for (const identity of option?.cards ?? decklistCards(STARTER_DECKLISTS[0]!)) {
    counts[identity] = (counts[identity] ?? 0) + 1;
  }
  return counts;
}

/**
 * Read this device's saved decks once (ADR 0027).
 *
 * Never load-bearing: the room renders completely with the store empty, which is
 * also what it does while the read is in flight and what it does forever if the
 * read fails (private-mode storage, a blocked transaction). A failure degrades to
 * the bundled starters and never reaches the screen.
 */
export function useSavedDecks(): SavedDeck[] {
  const [saved, setSaved] = useState<SavedDeck[]>([]);
  useEffect(() => {
    let live = true;
    listSavedDecks().then(
      (decks) => {
        if (live) setSaved(decks);
      },
      () => {
        /* Storage is unavailable; the starters are the whole library. */
      },
    );
    return () => {
      live = false;
    };
  }, []);
  return saved;
}
