/**
 * The bundled starter decks.
 *
 * `data/starter-decks.json` is shared game data owned by neither the Rust nor the client side,
 * and it is the same file the agent-vs-agent wire test plays. Reading it here rather than
 * restating it keeps the decks a client offers identical to the decks the engine is tested
 * against.
 *
 * Entries carry a `count`; the wire wants a flat list of identities, so expansion happens here.
 * This encodes no rules — the server validates every submitted deck against its own catalog and
 * the room's format.
 */
import starterDecks from '../../../data/starter-decks.json'

export interface StarterDeck {
  id: string
  name: string
  summary: string
  /** Every card identity, one entry per copy, in the order the deck lists them. */
  cards: readonly string[]
}

export const STARTER_DECKS: readonly StarterDeck[] = starterDecks.decks.map((deck) => ({
  id: deck.id,
  name: deck.name,
  summary: deck.summary,
  cards: deck.entries.flatMap((entry) => Array.from({ length: entry.count }, () => entry.identity)),
}))
