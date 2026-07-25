/**
 * The receiver's own hand — the baseline's broad, tactile curved fan (issue
 * #533), split out of `LiveMatchTable.tsx` so the shell composition and the
 * fan's own geometry/paging are separately readable.
 *
 * It is a **shell region**, not a scene object (ADR 0032 §7), and it shares its
 * curve with every opponent's face-down fan through `table/handFan.ts`: one
 * curve model, one overlap rule, one paging mechanism, two tiers. What differs
 * here is that every card is a real hit target, so the tier's exposure floor is
 * the normative 44 px of `presentation-budgets.md` §Accessibility.
 *
 * ## Geometry
 *
 * The band is `shellLayout.shellBands(viewport).hand`; #528's invariant I2 says
 * the fan's endpoints are inset by half a card so nothing can be clipped at any
 * band width. This component publishes that inset as `--hand-inset`, *widening*
 * it by the fan's centring slack (a small hand is a tight fan under the
 * medallion, not two cards pinned to opposite band edges) and by the page
 * gutter when the fan pages. The stylesheet's own `--hand-inset` declaration
 * stays as the documented default; an inline value only ever makes the inset
 * larger, so containment is strengthened, never weakened.
 *
 * ## Paging
 *
 * `layout-model.md` §Stress dispositions: the fan compresses spacing and
 * rotation before card size, and **pages** when exposed spacing would drop
 * below the 44 px floor, with ≥ 44 px page controls and the board still
 * visible. {@link localFanPlan} derives the page size from the floor, so the
 * floor is arithmetic rather than a target. The controls sit in gutters the fan
 * gives back when it pages, so a control can never cover a card.
 *
 * The page is **ephemeral presentation state**: it resets with every fresh
 * `GameView`, and it follows the selection and the active prompt's candidates,
 * so a candidate is never stranded behind a page the player has to find
 * (`layout-model.md` §Interaction guarantees — a pick is never removed).
 *
 * ## Parity
 *
 * Every gesture here has the non-drag equivalent `control-language.md` §7
 * requires: click selects, a second activation fires the sole action, `Enter`
 * on a focused card does the same through the spatial focus engine, touch taps,
 * and the page controls are ordinary buttons — reachable by pointer, by the
 * hand region's own arrow traversal, and by tap. Drag is only ever an
 * enhancement layered on `onPointerDown`.
 */
import {
  useEffect,
  useState,
  type CSSProperties,
  type PointerEvent as ReactPointerEvent,
} from 'react';
import { CardFace } from '../../card/dom';
import type { CardView, EntityId, GameView, PlayerId } from '../../protocol';
import {
  FAN,
  LOCAL_FAN_TIER,
  fanAngle,
  fanDip,
  fanFraction,
  fanInset,
  fanPageOf,
  fanPageRange,
  localFanPlan,
} from '../handFan';
import { domCardArt, handDisplayData } from '../planeDisplayData';
import styles from './live-match.module.css';

/** Everything the fan needs from the shell; all of it already derived there. */
export interface HandFanProps {
  /** The latest complete personalized view — the fan's sole source of cards. */
  view: GameView;
  /** The hand band's width in px (`shellLayout.shellBands(viewport).hand.w`). */
  bandWidth: number;
  /** Whether a targeting / multi-select session is open. */
  selecting: boolean;
  /** Whether the open session is a multi-select (toggling) one. */
  multiSelect: boolean;
  /** The active slot's candidate ids, straight from the server's enumeration. */
  candidates: readonly EntityId[];
  /** Candidates already chosen in the active slot. */
  chosen: readonly EntityId[];
  /** The current selection, if any. */
  selectedId: EntityId | null;
  /** The id currently previewed as a target, if any. */
  previewTargetId: EntityId | PlayerId | null;
  /** Whether a click should be swallowed because a drag just ended. */
  shouldSwallowClick: () => boolean;
  /** Pick (or toggle) a candidate. */
  onPick: (id: EntityId) => void;
  /** Activate a card that has offered actions (ADR 0025's select-then-fire). */
  onActivate: (id: EntityId) => void;
  /** Open the inspect surface on a card. */
  onInspect: (id: EntityId) => void;
  /** Preview a candidate as the pending target (hover/focus). */
  onPreviewTarget: (id: EntityId | null) => void;
  /** Arm the drag enhancement. Never the only path to any action. */
  onCardPointerDown: (card: CardView, event: ReactPointerEvent<HTMLButtonElement>) => void;
}

type HandStyle = CSSProperties & Record<`--${string}`, string | number>;

/** Render the receiver's curved hand fan into the shell's hand band. */
export function HandFan({
  view,
  bandWidth,
  selecting,
  multiSelect,
  candidates,
  chosen,
  selectedId,
  previewTargetId,
  shouldSwallowClick,
  onPick,
  onActivate,
  onInspect,
  onPreviewTarget,
  onCardPointerDown,
}: HandFanProps) {
  const hand = view.my_hand;
  const { plan, paged } = localFanPlan(hand.length, bandWidth);
  const [page, setPage] = useState(0);

  // The card the fan must be showing: the selection, else the first candidate
  // the active slot still wants. Everything about the page is derived from the
  // view plus this ephemeral state — a fresh mount reproduces it exactly.
  const pulled = hand.findIndex(
    (card) => card.id === selectedId || (candidates.includes(card.id) && !chosen.includes(card.id)),
  );

  useEffect(() => {
    setPage(0);
  }, [view]);

  useEffect(() => {
    if (pulled < 0) return;
    setPage((current) => {
      const wanted = fanPageOf(plan, pulled);
      return current === wanted ? current : wanted;
    });
    // `plan` is recomputed every render; its paging shape is what matters here.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pulled, plan.pageSize, plan.pages]);

  const current = Math.max(0, Math.min(page, Math.max(0, plan.pages - 1)));
  const { start, end } = fanPageRange(plan, current);
  const shown = hand.slice(start, end);
  const inset = fanInset(plan, LOCAL_FAN_TIER) + (paged ? FAN.pagerW : 0);

  const bandStyle: HandStyle = {
    '--hand-inset': `${inset}px`,
    '--hand-pager-w': `${FAN.pagerW}px`,
  };

  return (
    <div
      className={styles.hand}
      data-testid="live-hand"
      data-focus-region="hand"
      data-count={hand.length}
      data-count-band={plan.band}
      data-page={current}
      data-pages={plan.pages}
      style={bandStyle}
    >
      {/* The page controls bracket the cards in DOM order, so the spatial focus
          engine's along-axis walk of the hand region reads
          prev → card 0 … card n → next. Every drag-free path
          `control-language.md` §7 requires reaches them. */}
      {paged && (
        <>
          <button
            type="button"
            className={`${styles.handPage} ${styles.handPagePrev}`}
            data-testid="hand-page-prev"
            aria-label={`Previous hand page, page ${current} of ${plan.pages}`}
            disabled={current === 0}
            onClick={() => setPage(current - 1)}
          >
            ‹
          </button>
          <p className={styles.handPageLabel} data-testid="hand-page-label" aria-live="polite">
            {`Hand ${start + 1}–${end} of ${hand.length}`}
          </p>
        </>
      )}
      {shown.map((card, offset) => {
        const picking = candidates.includes(card.id);
        const playable = view.valid_actions.some((action) => action.subject?.includes(card.id));
        const cardStyle: HandStyle = {
          // Position along the fan as a 0…1 fraction of the band's usable span;
          // the stylesheet insets that span by `--hand-inset`, so the outermost
          // cards can never be clipped (shellLayout invariant I2).
          '--hand-t': fanFraction(offset, shown.length),
          '--hand-angle': `${fanAngle(offset, shown.length, LOCAL_FAN_TIER).toFixed(2)}deg`,
          '--hand-dip': `${fanDip(offset, shown.length, LOCAL_FAN_TIER).toFixed(2)}px`,
        };
        return (
          <button
            key={card.id}
            type="button"
            className={styles.handCard}
            data-testid={`live-hand-card-${card.id}`}
            data-entity={card.id}
            data-hand-index={start + offset}
            data-actionable={(!selecting && playable) || undefined}
            aria-label={
              picking
                ? `${multiSelect ? 'Toggle' : 'Target'} ${card.name}`
                : playable
                  ? `${card.name} — playable`
                  : `Inspect ${card.name}`
            }
            aria-pressed={
              picking && multiSelect ? chosen.includes(card.id) : selectedId === card.id
            }
            onClick={() => {
              if (shouldSwallowClick()) return;
              if (picking) onPick(card.id);
              else if (!selecting && playable) onActivate(card.id);
              else onInspect(card.id);
            }}
            onPointerEnter={() => {
              if (picking) onPreviewTarget(card.id);
            }}
            onPointerLeave={() => {
              if (previewTargetId === card.id) onPreviewTarget(null);
            }}
            onFocus={() => {
              if (picking) onPreviewTarget(card.id);
            }}
            onBlur={() => {
              if (previewTargetId === card.id) onPreviewTarget(null);
            }}
            onPointerDown={(event) => {
              if (selecting) return;
              onCardPointerDown(card, event);
            }}
            onContextMenu={(event) => {
              event.preventDefault();
              onInspect(card.id);
            }}
            style={cardStyle}
          >
            {/* `hand` is a full-card tier, so its face has a rules area
                (card-representation §3.2). The server's `rules_text` is the
                only thing that may fill it — omitting the prop blanks the
                rules on every card in hand. */}
            <CardFace
              data={handDisplayData(view, card)}
              tier="hand"
              art={domCardArt(card)}
              rulesText={card.rules_text}
            />
          </button>
        );
      })}
      {paged && (
        <button
          type="button"
          className={`${styles.handPage} ${styles.handPageNext}`}
          data-testid="hand-page-next"
          aria-label={`Next hand page, page ${current + 2} of ${plan.pages}`}
          disabled={current >= plan.pages - 1}
          onClick={() => setPage(current + 1)}
        >
          ›
        </button>
      )}
    </div>
  );
}
