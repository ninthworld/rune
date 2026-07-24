/**
 * Host-only AI seating (issue #415), moved under the roster where it belongs —
 * it is a roster operation, not a deck operation (`front-door-and-lobby.md`
 * §5.3, last bullet).
 *
 * Pick an open seat, an AI kind (from the server-advertised
 * `CatalogView.ai_opponents`), and a deck, then seat it. Rendered **only** when
 * the server offers `add_ai`, so host-ness is never inferred client-side, and
 * the deck the host picks is validated authoritatively server-side exactly like
 * a human's.
 */
import { useState } from 'react';
import { cx } from '../chrome/cx';
import { STARTER_DECKLISTS, decklistById, type Decklist } from '../decklists';
import type { AiOption, RoomView } from '../protocol';
import { DeckGrid } from './DeckPicker';
import { commanderName } from './deckPresentation';
import p from './styles';

export function AiSeatingCard({
  room,
  aiOptions,
  requiresCommander,
  reducedMotion,
  onAddAi,
}: {
  room: RoomView;
  aiOptions: readonly AiOption[];
  requiresCommander: boolean;
  reducedMotion: boolean;
  onAddAi: (seat: number, kind: string, deck: Decklist) => void;
}) {
  const openSeats = room.seats
    .filter((seat) => seat.occupied_by === undefined && seat.ai === undefined)
    .map((seat) => seat.seat);
  const [seat, setSeat] = useState<number | undefined>(openSeats[0]);
  const [kind, setKind] = useState(aiOptions[0]?.id);
  const [deckId, setDeckId] = useState(STARTER_DECKLISTS[0]!.id);

  // The picked seat/kind must stay valid as the roster and catalog change (a
  // seat filled by someone else, or the catalog arriving) — reconstruct-from-
  // one-view: nothing here is load-bearing, so fall back to the first still-
  // valid choice.
  const seatValue = seat !== undefined && openSeats.includes(seat) ? seat : openSeats[0];
  const kindValue = aiOptions.some((option) => option.id === kind) ? kind : aiOptions[0]?.id;

  if (openSeats.length === 0 || aiOptions.length === 0 || kindValue === undefined) return null;

  const deck = decklistById(deckId) ?? STARTER_DECKLISTS[0]!;
  return (
    <section className={p.panel} aria-label="Add an AI opponent" data-testid="ai-seating">
      <h2 className={p.title}>Add an AI opponent</h2>
      <div className={p.group} role="group" aria-label="Seat for the AI opponent">
        <span className={p.fieldLabel}>Seat</span>
        <div className={p.segmentRow}>
          {openSeats.map((index) => (
            <button
              key={index}
              type="button"
              className={cx(p.segment, index === seatValue && p.segmentOn)}
              aria-pressed={index === seatValue}
              onClick={() => setSeat(index)}
              data-testid={`ai-seat-${index}`}
            >
              {index + 1}
            </button>
          ))}
        </div>
      </div>
      <div className={p.group} role="group" aria-label="AI opponent kind">
        <span className={p.fieldLabel}>Opponent</span>
        <div className={p.segmentRow}>
          {aiOptions.map((option) => (
            <button
              key={option.id}
              type="button"
              className={cx(p.segment, option.id === kindValue && p.segmentOn)}
              aria-pressed={option.id === kindValue}
              onClick={() => setKind(option.id)}
              title={option.description}
              data-testid={`ai-kind-${option.id}`}
            >
              {option.name}
            </button>
          ))}
        </div>
      </div>
      <DeckGrid
        decks={STARTER_DECKLISTS}
        selectedId={deckId}
        onSelect={setDeckId}
        reducedMotion={reducedMotion}
        label="AI deck"
      />
      {requiresCommander && commanderName(deck) !== undefined && (
        <span className={p.muted}>AI commander: {commanderName(deck)}</span>
      )}
      <div className={p.buttonRow}>
        <button
          type="button"
          className={p.button}
          onClick={() => seatValue !== undefined && onAddAi(seatValue, kindValue, deck)}
          data-testid="add-ai-button"
        >
          Add AI to seat {(seatValue ?? 0) + 1}
        </button>
      </div>
    </section>
  );
}
