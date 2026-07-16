/**
 * Bundled starter decklists — static, display-only data (issue #114 scope).
 *
 * These are *names and card identities only*. They carry **no card logic**: the
 * client never reads a card's cost, type, rules text, or legality from here — the
 * server validates a submitted decklist authoritatively against its card database
 * (ADR 0012) and the engine owns every rule. This module exists solely so the
 * lobby can offer a player a couple of ready-made decks to submit without a deck
 * builder (an explicit non-goal for this issue).
 *
 * A decklist is expressed as `(card, count)` rows for readable display; the wire
 * form is a flat list of {@link CardIdentity} handles (a card repeated `count`
 * times), assembled by {@link decklistCards}. An identity is a card's authored
 * `functional_id` (ADR 0018 §3) — the only card identity that is stable across
 * builds. It is *not* the engine's `CardId`: that handle is interned from the
 * catalog's sort order, so authoring one new card renumbers its neighbours, and a
 * decklist pinned to an integer would quietly come to mean different cards.
 *
 * The client still never parses an identity — it is an opaque string it echoes back.
 * The server remains the sole authority on whether a given deck is legal for a given
 * `game_setup`.
 */
import starterDecks from './starter-decks.json';
import type { CardIdentity } from './protocol';

/** One line of a decklist: a card identity, its display name, and a count. */
export interface DeckEntry {
  /** Opaque card-identity handle submitted to the server (never parsed here). */
  readonly identity: CardIdentity;
  /** Display name shown in the lobby — presentation only, never a rule input. */
  readonly name: string;
  /** How many copies of this card the deck runs. */
  readonly count: number;
}

/** A bundled starter deck: a stable id, a display name, and its card rows. */
export interface Decklist {
  /** Stable local id used to key the lobby's deck selector (never sent). */
  readonly id: string;
  /** Display name shown to the player. */
  readonly name: string;
  /** One-line flavor/summary shown under the name; presentation only. */
  readonly summary: string;
  /** The deck's `(identity, name, count)` rows. */
  readonly entries: readonly DeckEntry[];
}

/**
 * The bundled starter decks. Built from the engine's catalog
 * (`crates/rune-engine/data/catalog/<functional_id>.json`) referenced by identity
 * only — this is static data, not a card database, and encodes no rules.
 *
 * The list is loaded from `starter-decks.json`, the **single source of truth** shared
 * with the engine's agent-vs-agent wire test (`crates/rune-cli/tests/agent_game.rs`),
 * which reads that same file and plays every deck to completion through the real
 * server. Keeping the client's decks and the test's decks in one file is what stops
 * them from silently drifting apart (issue #257). Edit the decks there, not here.
 */
export const STARTER_DECKLISTS: readonly Decklist[] = starterDecks.decks;

/** Look a bundled decklist up by its local id, or `undefined` if unknown. */
export function decklistById(id: string): Decklist | undefined {
  return STARTER_DECKLISTS.find((deck) => deck.id === id);
}

/**
 * Expand a decklist into the flat list of card identities the wire carries — each
 * entry repeated `count` times. This is pure data assembly (no legality); the
 * server validates the result.
 */
export function decklistCards(deck: Decklist): CardIdentity[] {
  const cards: CardIdentity[] = [];
  for (const entry of deck.entries) {
    for (let i = 0; i < entry.count; i += 1) cards.push(entry.identity);
  }
  return cards;
}

/** Total number of cards in a decklist (for display). */
export function decklistSize(deck: Decklist): number {
  return deck.entries.reduce((total, entry) => total + entry.count, 0);
}
