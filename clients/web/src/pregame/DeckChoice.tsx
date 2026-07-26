/**
 * The room's deck choice, as the approved game-lobby baseline draws it (issue
 * #546): **one dropdown on the local seat**, never every available deck at once.
 *
 * #506 rendered the whole starter library as a tile grid in a right-hand column,
 * which is the "deck library as a permanent panel" the baseline replaces. The
 * library itself is not lost — it is contextual: `deckChoice.ts` assembles the
 * bundled starters plus whatever the player has saved on this device (ADR 0027),
 * and the quiet Edit/Import shortcuts beside this dropdown open the builder,
 * which owns that interface (#508 owns its visual system, deliberately out of
 * scope here).
 *
 * No card logic, and choosing a deck here claims nothing about its legality:
 * every submission is validated authoritatively server-side behind the unchanged
 * `submit_deck` gate.
 */
import type { DeckChoiceOption } from './deckChoice';
import p from './styles';

export interface DeckChoiceProps {
  options: readonly DeckChoiceOption[];
  selectedId: string;
  onSelect: (id: string) => void;
}

/** The dropdown itself. One press, one list, no grid. */
export function DeckChoice({ options, selectedId, onSelect }: DeckChoiceProps) {
  return (
    <label className={p.seatOptionsField}>
      <span className={p.fieldLabel}>Deck</span>
      <select
        className={p.deckSelect}
        value={selectedId}
        onChange={(event) => onSelect(event.target.value)}
        data-testid="deck-select"
      >
        {options.map((option) => (
          <option key={option.id} value={option.id} data-testid={`deck-option-${option.id}`}>
            {option.name}
          </option>
        ))}
      </select>
    </label>
  );
}
