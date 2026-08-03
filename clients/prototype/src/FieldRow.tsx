import type { CardData } from "./Card";
import { Permanent, type Perm } from "./Permanent";
import { useScrollStrip } from "./scrollStrip";

/* A row of permanents. Cards are sized from the row's height, so a seat
   that is short and narrow — eight players, or a phone — runs out of
   width long before it runs out of permanents, and the row scrolls to
   reach the rest. */
export function FieldRow({
  perms,
  onHover,
  anchor,
}: {
  perms: Perm[];
  onHover?: (card: CardData | null) => void;
  anchor?: string;
}) {
  const { ref, edges } = useScrollStrip<HTMLDivElement>();
  return (
    <div className="field-row">
      <div className={`strip field-scroll${edges}`} ref={ref}>
        {perms.map((perm, i) => (
          <Permanent
            key={i}
            perm={perm}
            onHover={onHover}
            anchor={anchor && `${anchor}:${i}`}
          />
        ))}
      </div>
    </div>
  );
}
