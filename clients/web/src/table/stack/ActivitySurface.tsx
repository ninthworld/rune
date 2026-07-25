/**
 * The **activity surface** — the badge, the auto-surfaced ticker, and the door to
 * the full history (issue #534, under
 * [ADR 0032](../../../../../docs/decisions/0032-contextual-shell-anatomy.md)).
 *
 * ## The contract with integration
 *
 * Mount it as a **direct child of the match shell root**, a sibling of the shell
 * regions, exactly like {@link StackStage} — and for the same two reasons: a
 * region's z-index would trap the history overlay below `--rune-z-decision`, and
 * a region would give an empty log a track to reserve.
 *
 * ```tsx
 * <ActivitySurface
 *   view={view}                       // the latest GameView
 *   onHighlight={highlight}           // the shipped presentational highlight
 *   highlightedId={highlightedId}
 * />
 * ```
 *
 * Both handlers are the ones `Rail.tsx` forwarded to {@link GameLog}, unchanged.
 * The unread marker is **not** a prop: the surface owns `useUnreadLog` itself, so
 * integration has one fewer thing to keep in sync and the badge cannot disagree
 * with the panel it opens.
 *
 * ## Three costs, not one column
 *
 * ADR 0023 gave the log a permanent column; ADR 0032 takes it back. What
 * survives is graded by how much a reader needs it:
 *
 * - the **badge** is one 44 ⌀ icon button — the explicit, always-available way to
 *   open the full history, which is the acceptance criterion "preserve an
 *   explicit way to inspect full history";
 * - the **ticker** surfaces the newest meaningful lines on its own and dwells out
 *   again, which is "activity when needed for comprehension" without a column;
 * - the **history** is the shipped {@link GameLog} panel, composed rather than
 *   reimplemented, so grouping, auto-scroll, the unread affordance, and the
 *   clickable references all behave exactly as they did in the rail.
 *
 * ## Layering
 *
 * The badge and ticker sit at `--rune-z-shell` (they are chrome and must never
 * cover a decision). The history sits at `--rune-z-overlay`, which it is entitled
 * to because the player invoked it and one `Escape` dismisses it without
 * answering anything — ADR 0032's rule stated exactly.
 */
import { useEffect, useState } from 'react';
import type { EntityId, GameView } from '../../protocol';
import { cx } from '../../chrome/cx';
import { GameLog } from '../GameLog';
import { isRef, type LogSegment } from '../logComposition';
import { useUnreadLog } from '../useUnreadLog';
import { IconButton } from '../controls';
import { ACTIVITY, deriveActivity, newestSequence, type ActivityLine } from './activityFeed';
import { stackStyleVars } from './stackStage';
import s from './stack.module.css';

/** The id the badge's `aria-controls` points at. */
const HISTORY_ID = 'stack-activity-history';

export interface ActivitySurfaceProps {
  /** The latest view; the surface renders exactly its `log` window. */
  view: GameView;
  /**
   * Presentationally highlight a reference's object on the table (issue #260).
   * Omitted in read-only contexts, which makes references plain text.
   */
  onHighlight?: (id: EntityId) => void;
  /** The id currently highlighted, so its references read pressed. */
  highlightedId?: EntityId | null;
}

/**
 * Render one composed line's segments. The words are {@link describeEvent}'s;
 * this only decides which of them are buttons — the same split `GameLog` makes,
 * repeated here rather than shared because the panel's markup is a list of `<li>`
 * and the ticker's is a list of spans.
 */
function renderSegments(
  segments: LogSegment[],
  onHighlight: ActivitySurfaceProps['onHighlight'],
): React.ReactNode {
  return segments.map((segment, i) => {
    if (!isRef(segment)) return <span key={i}>{segment}</span>;
    if (onHighlight === undefined) return <span key={i}>{segment.name}</span>;
    return (
      <button
        key={i}
        type="button"
        className={s.ref}
        data-testid={`activity-ref-${segment.id}`}
        aria-label={`Highlight ${segment.name}`}
        onClick={() => onHighlight(segment.id)}
      >
        {segment.name}
      </button>
    );
  });
}

export function ActivitySurface({ view, onHighlight, highlightedId }: ActivitySurfaceProps) {
  const { unreadCount, isUnseen, markSeen } = useUnreadLog(view.log ?? []);
  const [historyOpen, setHistoryOpen] = useState(false);
  // The ticker's dwell watermark: entries at or below it have had their moment.
  // Ephemeral presentation — a fresh mount starts at -1 so a player who just
  // reconnected sees the newest lines once, which is when they need them most.
  const [since, setSince] = useState(-1);

  const newest = newestSequence(view);
  const model = deriveActivity(view, { unreadCount, isUnseen, sinceSequence: since });
  const surfacedCount = model.surfaced.length;

  useEffect(() => {
    if (surfacedCount === 0) return undefined;
    const timer = setTimeout(() => setSince(newest), ACTIVITY.dwellMs);
    return () => clearTimeout(timer);
  }, [newest, surfacedCount]);

  useEffect(() => {
    if (!historyOpen) return undefined;
    const onKey = (event: KeyboardEvent): void => {
      if (event.key === 'Escape') setHistoryOpen(false);
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [historyOpen]);

  if (!model.present) return null;

  const openHistory = (): void => {
    setHistoryOpen(true);
    // Opening the history IS the "I am reading the new activity" gesture (#340),
    // and it retires the ticker, which has just been superseded by the panel.
    markSeen();
    setSince(newest);
  };

  const line = (entry: ActivityLine) => (
    <li
      key={entry.sequence}
      className={cx(s.tickerLine, entry.unseen && s.tickerLineUnseen)}
      data-testid={`activity-line-${entry.sequence}`}
    >
      {entry.unseen && <span className={s.srOnly}>New: </span>}
      {renderSegments(entry.segments, onHighlight)}
    </li>
  );

  return (
    <>
      <div className={s.activity} style={stackStyleVars()} data-testid="activity-surface">
        {/* One `aria-live` region for activity, and only while the panel that
            would otherwise announce the same lines is closed. */}
        {!historyOpen && surfacedCount > 0 && (
          <ul className={s.ticker} data-testid="activity-ticker" aria-live="polite">
            {model.surfaced.map(line)}
          </ul>
        )}
        <IconButton
          glyph={model.badgeText}
          label={model.badgeLabel}
          onPress={openHistory}
          expanded={historyOpen}
          controls={HISTORY_ID}
          testId="activity-badge"
        />
      </div>
      {historyOpen && (
        <div className={s.history} data-testid="activity-history">
          <section className={s.historyPanel} id={HISTORY_ID} aria-label="Full game history">
            <div className={s.historyHead}>
              <h2 className={s.historyTitle}>History</h2>
              <IconButton
                glyph="×"
                label="Close the full history"
                onPress={() => setHistoryOpen(false)}
                testId="activity-history-close"
              />
            </div>
            <div className={s.historyBody}>
              {/* The shipped log panel, unchanged: grouping, auto-scroll pin,
                  unread marker, and clickable references all carried. */}
              <GameLog
                view={view}
                onHighlight={onHighlight}
                highlightedId={highlightedId}
                isUnseen={isUnseen}
                unreadCount={unreadCount}
                onSeen={markSeen}
              />
            </div>
          </section>
        </div>
      )}
    </>
  );
}
