import type { CSSProperties } from "react";
import { Card, type CardData, type Counter } from "./Card";

/* A permanent occupies one slot on a battlefield row. It may be a single
   card, a pile of identical ones, or a card with things attached — and it
   may be tapped, which turns the whole slot.

   The two piles are told apart by which way they cascade rather than by
   any extra chrome: copies of the same permanent fall straight down, so
   the eye reads a column of identical title bars and counts them, while
   an attachment steps down *and* to the right, so an equipped creature
   never reads as two unrelated piles sitting next to each other. Either
   way the pile is fitted to the row's height, so a stack of four lands is
   exactly as tall as the single creature beside it.

   What is on top differs too. In a pile of copies each card covers the
   one before, leaving a ladder of title bars. Attachments go behind the
   thing they are attached to, so the creature — the permanent that
   attacks, blocks and dies — is the whole card, and the Equipment shows
   only its title. */
export type Perm = {
  card: CardData;
  tapped?: boolean;
  copies?: number;
  attached?: CardData[];
  /* what is on the permanent rather than printed on the card — worn by
     the top card of the slot, which is the permanent itself */
  counters?: Counter[];
  sick?: boolean;
};

/* how far each card steps, as a fraction of a card. A pile only has to
   be countable, so it spends less than an attachment, which has to stay
   readable. */
const PILE_STEP = 0.11;
const ATTACH_STEP = 0.16;

export function Permanent({
  perm,
  onHover,
  anchor,
}: {
  perm: Perm;
  onHover?: (card: CardData | null) => void;
  anchor?: string;
}) {
  const { card, tapped, copies = 1, attached } = perm;
  /* the attached cards are laid down first so the creature lands on top */
  const cards = attached?.length
    ? [...attached, card]
    : Array.from({ length: copies }, () => card);
  const step = attached?.length ? ATTACH_STEP : PILE_STEP;
  const style = {
    "--n": cards.length,
    "--dx": attached?.length ? step : 0,
    "--dy": cards.length > 1 ? step : 0,
  } as CSSProperties;
  return (
    <div className={`perm${tapped ? " perm-tapped" : ""}`} style={style}>
      {/* an arrow aims at the inner box: turned by the same rotation as the
          permanent, so a tapped card takes its ring lying down */}
      <div className="perm-inner" data-anchor={anchor}>
        {cards.map((c, i) => (
          <Card
            key={i}
            card={c}
            onHover={onHover}
            style={{ "--i": i } as CSSProperties}
            counters={i === cards.length - 1 ? perm.counters : undefined}
            sick={i === cards.length - 1 ? perm.sick : undefined}
          />
        ))}
      </div>
    </div>
  );
}
