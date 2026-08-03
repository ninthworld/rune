import { Card, type CardData } from "./Card";
import { useScrollStrip } from "./scrollStrip";

/* The hand centres itself while it fits and scrolls once it doesn't —
   the same strip the battlefield rows use, so reaching a card off the
   end is the same gesture wherever it is. */
export function Hand({
  cards,
  onHover,
}: {
  cards: CardData[];
  onHover?: (card: CardData | null) => void;
}) {
  const { ref, edges } = useScrollStrip<HTMLDivElement>();
  return (
    <div className="hand">
      <div className={`strip hand-scroll${edges}`} ref={ref}>
        {cards.map((card, i) => (
          <Card key={card.name} card={card} onHover={onHover} anchor={`hand:${i}`} />
        ))}
      </div>
    </div>
  );
}
