import { useEffect, useState } from "react";
import { Card, type CardData } from "./Card";

/* Looking inside a zone. One surface serves every case the game has —
   reading a graveyard or an exile, checking a command zone, searching a
   library — because they differ only in what they are called and whether
   the game wants a card back. `choose` is what splits them: without it
   this is something you read and close; with it, one card comes back and
   the footer names the act in the game's words ("Put into hand"), not
   the UI's ("OK").

   The grid fills by card width rather than a fixed column count, so the
   same panel is four across on a desktop and two on a phone without a
   second layout to keep in step. */
export type ZoneRequest = {
  title: string;
  cards: CardData[];
  choose?: { action: string; hint: string };
};

export function ZoneView({
  request,
  onClose,
  onChoose,
}: {
  request: ZoneRequest;
  onClose: () => void;
  onChoose?: (card: CardData) => void;
}) {
  const [picked, setPicked] = useState<number | null>(null);
  const { title, cards, choose } = request;

  useEffect(() => setPicked(null), [request]);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  return (
    <div className="zone-view" onClick={onClose}>
      <div
        className={`zone-panel${choose ? " zone-choosing" : ""}`}
        onClick={(e) => e.stopPropagation()}
      >
        <div className="zone-head">
          <span className="zone-title">{title}</span>
          <span className="zone-tally">
            {cards.length} {cards.length === 1 ? "card" : "cards"}
          </span>
          <button className="zone-close" onClick={onClose} title="Close">
            ✕
          </button>
        </div>

        <div className="zone-body">
          {cards.length === 0 && <p className="zone-empty">Nothing here.</p>}
          {cards.map((card, i) => (
            <button
              key={i}
              className={`zone-slot${picked === i ? " picked" : ""}`}
              onClick={choose ? () => setPicked(i) : undefined}
              onDoubleClick={
                choose && onChoose ? () => onChoose(card) : undefined
              }
            >
              <Card card={card} />
            </button>
          ))}
        </div>

        {choose && (
          <div className="zone-foot">
            <span className="zone-hint">
              {picked === null ? choose.hint : cards[picked].name}
            </span>
            <div className="zone-acts">
              <button className="action-done action-alt" onClick={onClose}>
                Cancel
              </button>
              <button
                className="action-done"
                disabled={picked === null}
                onClick={() =>
                  picked !== null && onChoose && onChoose(cards[picked])
                }
              >
                {choose.action}
              </button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
