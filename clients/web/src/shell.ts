/**
 * The destinations of the client shell, and the rule that makes them safe.
 *
 * **Which destination you are on is the client's answer. Which contract you are on is the
 * server's.** A `GameView` arriving replaces the whole shell, because the contract changed;
 * choosing Decks does not, because it did not. That distinction is what keeps "no client-held
 * phase" true while still letting a player walk to their deck locker and back — a phase is a
 * second opinion about what the server said, and a destination is not an opinion about the
 * server at all.
 *
 * Nothing here is derived from a view. The list is fixed, in this order, at every width: at
 * narrow widths the rail becomes a bar and the destinations and their order do not change, which
 * is what makes the shell learnable on a phone and a desktop as one thing rather than two.
 */

export type Destination = 'play' | 'decks' | 'settings'

export interface DestinationEntry {
  id: Destination
  label: string
  /** Drawn beside the label; the label is what is read, and this is never the only copy. */
  glyph: string
}

export const DESTINATIONS: readonly DestinationEntry[] = [
  { id: 'play', label: 'Play', glyph: '▣' },
  { id: 'decks', label: 'Decks', glyph: '❐' },
  { id: 'settings', label: 'Settings', glyph: '⚙' },
]
