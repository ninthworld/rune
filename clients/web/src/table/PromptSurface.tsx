/**
 * The prompt surface: a React DOM list overlay for the two prompt shapes whose
 * candidates are not laid out as canvas cards (issue #157, ADR 0003 — text a user
 * reads/clicks is DOM, not canvas):
 *
 * - **select** — a `select_from_zone` whose zone is not on the board (graveyard,
 *   library, exile): each candidate is a toggle row, pressed when chosen. The
 *   hand/battlefield case stays on the canvas (candidates highlight in place); this
 *   overlay is only used when the zone isn't visible there.
 * - **order** — an `order` arrange list: every item is a row with move-up/down
 *   controls; reordering rows is the whole interaction (functional reordering, not
 *   drag polish — that is out of scope per the issue).
 *
 * Zero legality lives here: the rows, their order, and the chosen flags all come
 * from the server-issued prompt via the multi-select session. The component only
 * renders them and reports toggles/moves back; it never derives what is legal.
 */
import type { EntityId } from '../protocol';
import { cx } from '../chrome/cx';
import s from './chrome.module.css';

/** One row the surface renders: an entity id with its display label. */
export interface PromptSurfaceItem {
  /** The entity id echoed back in the answer. */
  id: EntityId;
  /** Human-readable label (the card name, or the id when no card is known). */
  label: string;
  /** Whether this candidate is currently chosen (select mode only). */
  chosen: boolean;
}

interface Props {
  /** `select` toggles candidates; `order` arranges the items via move controls. */
  mode: 'select' | 'order';
  /** The server's slot prompt, shown as the surface heading. */
  prompt: string;
  /** The originating zone label for context (select mode), e.g. `"graveyard"`. */
  zone?: string;
  /**
   * The rows to render. For `select`, the slot's candidates (with `chosen`); for
   * `order`, the items in their current arranged order (top = first).
   */
  items: PromptSurfaceItem[];
  /** Toggle a candidate into/out of the selection (select mode). */
  onToggle?: (id: EntityId) => void;
  /** Move an item one step earlier (`-1`) or later (`+1`) in the order (order mode). */
  onMove?: (id: EntityId, direction: -1 | 1) => void;
}

export function PromptSurface({ mode, prompt, zone, items, onToggle, onMove }: Props) {
  return (
    <section data-testid="prompt-surface" className={s.promptSurface} aria-label={prompt}>
      <h2 className={s.promptSurfaceTitle}>{prompt}</h2>
      {mode === 'select' && zone !== undefined && (
        <span className={s.promptSurfaceZone}>{zone}</span>
      )}
      <ul className={s.promptSurfaceList}>
        {items.map((item, index) => {
          const rowClass =
            mode === 'select' && item.chosen
              ? cx(s.promptSurfaceRow, s.promptSurfaceRowChosen)
              : s.promptSurfaceRow;
          return (
            <li key={item.id} className={rowClass}>
              {mode === 'order' && <span className={s.promptSurfaceIndex}>{index + 1}</span>}
              <span className={s.promptSurfaceName}>{item.label}</span>
              {mode === 'select' ? (
                <button
                  type="button"
                  onClick={() => onToggle?.(item.id)}
                  aria-pressed={item.chosen}
                  data-testid={`zone-select-${item.id}`}
                  className={s.promptSurfaceControl}
                >
                  {item.chosen ? 'Chosen' : 'Choose'}
                </button>
              ) : (
                <>
                  <button
                    type="button"
                    onClick={() => onMove?.(item.id, -1)}
                    disabled={index === 0}
                    aria-label={`Move ${item.label} up`}
                    data-testid={`order-up-${item.id}`}
                    className={s.promptSurfaceControl}
                  >
                    ↑
                  </button>
                  <button
                    type="button"
                    onClick={() => onMove?.(item.id, 1)}
                    disabled={index === items.length - 1}
                    aria-label={`Move ${item.label} down`}
                    data-testid={`order-down-${item.id}`}
                    className={s.promptSurfaceControl}
                  >
                    ↓
                  </button>
                </>
              )}
            </li>
          );
        })}
      </ul>
    </section>
  );
}
