/**
 * The **mana reservoir** — the receiver's floating mana, drawn in the lower-right
 * action area (issue #567, absorbing #462's remaining scope).
 *
 * Floating mana had no home on the 2.5D surface at all: `view.mana_pool` reached
 * the client, was normalised by `wire.ts`, and was then drawn by exactly one
 * component — `MePanel`, a survivor of ADR 0023's bottom shell that ADR 0032
 * retired and that nothing imported any more. A player could tap a land and see
 * nothing change. This is that value's home, beside the controls that spend it.
 *
 * ## Presentation only
 *
 * The reservoir reads `view.mana_pool` and draws it. It never sums, converts,
 * infers what a cost would leave, or decides that a spell is affordable — mana
 * state and spending stay server-authoritative, and paying is still whatever the
 * server offered in `valid_actions`. There is no protocol change here and no
 * client-side pool arithmetic; the array's order is the server's order.
 *
 * ## Symbols
 *
 * Drawing goes through the shared {@link SymbolText}, which is the one DOM
 * surface component for `{…}` notation (#462): each symbol is a `PIP`-swatched
 * disc with `role="img"` and the vocabulary's spoken name, so a reader hears
 * "green mana" rather than a letter, and a code the vocabulary does not know
 * renders as its own literal text rather than disappearing. Writing a second
 * renderer here would be how the two vocabularies drift apart.
 *
 * ## The receiver's pool alone
 *
 * `GameView` carries no opponent pool and the seat clusters deliberately show
 * none (`seat-identity.md` §5.2, pinned by `seat-cluster.test.ts`). This surface
 * reads `mana_pool` — the receiver's own — and nothing else.
 *
 * ## Empty is absent
 *
 * An empty pool renders nothing. ADR 0032's rule is that contextual chrome
 * "appears where and when it is relevant and is otherwise absent", and an empty
 * reservoir is a permanent widget saying zero — the kind of always-there dashboard
 * the ADR removed. The acceptance criterion is "always visible **when present**";
 * mana is present exactly when the server sent some.
 */
import { SymbolText, symbolNotationText } from '../../chrome/symbols';
import s from './cluster.module.css';

export interface ManaReservoirProps {
  /**
   * `GameView.mana_pool` — the receiver's floating mana, in the server's order,
   * each entry a symbol string such as `"{G}"`. Rendered verbatim.
   */
  pool: string[];
}

export function ManaReservoir({ pool }: ManaReservoirProps) {
  if (pool.length === 0) return null;

  return (
    <div
      className={s.reservoir}
      data-testid="mana-reservoir"
      // The drawn pips are `role="img"`, so the group needs the spoken
      // substitution or a reader hears a label with holes in it. Commas so the
      // symbols are read as a list rather than run together.
      role="group"
      aria-label={`Mana pool: ${symbolNotationText(pool.join(', '))}`}
    >
      <span className={s.reservoirTag} aria-hidden="true">
        Mana
      </span>
      <span className={s.reservoirPool} data-testid="mana-reservoir-pool">
        {/* One pass over the whole pool, so an unknown code falls back to its
            literal text through the same tokenizer the rest of the shell uses. */}
        <SymbolText text={pool.join('')} symbolClassName={s.reservoirSymbol} />
      </span>
    </div>
  );
}
