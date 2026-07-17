/**
 * The stack & activity rail (React DOM, issue #299): the right-edge home for the
 * stack and the game log (issue #260), with the public-zone activity feed reserved
 * for later.
 *
 * The rail has two presentations, and which one shows is a PURE function of the
 * latest view plus the measured geometry, so a fresh mount from one `GameView`
 * always resolves the correct default (nothing here is load-bearing across
 * messages, per AGENTS.md):
 *
 * - **expanded panel** — the stack panel (present only when the stack is populated)
 *   above the game log, docked in the rail column on wide geometry or floated over the
 *   board's right edge when the geometry is too narrow to dock. This is the default
 *   whenever there is activity and the geometry is not collapsed.
 * - **collapsed badge** — a single touch target showing the live object count (stack
 *   when populated, else the log window). This is the default on narrow geometry
 *   (`layout()`'s `railCollapsed`, geometry-driven — never a hardcoded breakpoint), so
 *   the board keeps its width.
 *
 * Manual expand/collapse is ephemeral presentation state (`override`), reset on
 * every fresh view so a reconnect/replay reconstructs the default. With neither a
 * populated stack nor any log the rail renders NOTHING — it then claims no
 * meaningful width.
 *
 * The stack itself is rendered by {@link StackPanel} unchanged: resolution order
 * top-first, the "resolves next" emphasis, controller/source meta, inspect handles,
 * and targeting-mode pickability of stack objects all come from it. The rail only
 * frames it and adds the collapse/expand affordance.
 */
import { useEffect, useState } from 'react';
import type { EntityId, GameView } from '../protocol';
import { cx } from '../chrome/cx';
import { GameLog } from './GameLog';
import { StackPanel } from './StackPanel';
import type { Rect } from './scene';
import { railBadgeBox, railFloat, regionBox } from './styles';
import s from './chrome.module.css';

/** The active target slot's stack-object candidates plus the pick handler, forwarded
 * verbatim to {@link StackPanel} so a stack object stays pickable in targeting mode. */
interface TargetingStack {
  candidates: EntityId[];
  onPick: (id: EntityId) => void;
}

interface Props {
  /** The latest view; the rail renders exactly its `stack`. */
  view: GameView;
  /** The rail region rect from `layout()` — the docked column when wide, or the
   * 44px badge anchor when the geometry collapsed the rail. */
  rect: Rect;
  /** Whether the geometry collapsed the rail to a badge (`layout()`'s `railCollapsed`,
   * NOT a hardcoded breakpoint). Drives the default (badge) and where an expanded
   * panel sits (floating vs docked). */
  collapsed: boolean;
  /** Present only in targeting mode; makes candidate stack objects pickable. */
  targeting?: TargetingStack;
  /** Open the inspect popover for a stack object (issue #261). */
  onInspect?: (id: EntityId) => void;
  /** Presentationally highlight a log reference's object on the table (issue #260).
   * Omitted in read-only contexts. */
  onHighlight?: (id: EntityId) => void;
  /** The id currently highlighted, forwarded to the log so its references read pressed. */
  highlightedId?: EntityId | null;
}

export function Rail({
  view,
  rect,
  collapsed,
  targeting,
  onInspect,
  onHighlight,
  highlightedId,
}: Props) {
  // Manual expand/collapse: an ephemeral override of the geometry default, reset on
  // every fresh view (like every other selection in the table) so the rail is always
  // reconstructable from one GameView + geometry. `null` ⇒ follow the default.
  const [override, setOverride] = useState<boolean | null>(null);
  useEffect(() => setOverride(null), [view]);

  const count = view.stack.length;
  const logCount = view.log?.length ?? 0;
  // Nothing on the stack AND no log ⇒ no rail chrome at all; it claims no meaningful
  // width. The log alone is enough to keep the activity rail present (its reserved
  // right-edge column is already carved by `layout()`, so filling it moves nothing).
  if (count === 0 && logCount === 0) return null;

  // Default expanded when there is activity AND the geometry is not collapsed; a manual
  // toggle overrides until the next view. This is what auto-expands the rail on stack
  // activity (wide) yet keeps the badge on narrow geometry.
  const expanded = override ?? !collapsed;

  if (!expanded) {
    // The badge counts the stack when populated, else the log window — either way it is
    // a single touch target that expands the activity rail.
    const badgeCount = count > 0 ? count : logCount;
    const label =
      count > 0
        ? `Stack: ${count} object${count === 1 ? '' : 's'} — expand activity rail`
        : `Game log: ${logCount} entr${logCount === 1 ? 'y' : 'ies'} — expand activity rail`;
    return (
      <button
        type="button"
        className={s.railBadge}
        style={railBadgeBox(rect)}
        data-testid="rail-badge"
        aria-label={label}
        aria-expanded={false}
        onClick={() => setOverride(true)}
      >
        <span className={s.railBadgeCount}>{badgeCount}</span>
      </button>
    );
  }

  return (
    <div
      className={cx(s.rail, collapsed ? s.railFloating : s.railDocked)}
      style={collapsed ? railFloat(rect) : regionBox(rect)}
      data-testid="rail"
      data-expanded="true"
    >
      <div className={s.railHeader}>
        <span className={s.railHeaderTitle}>Activity</span>
        <button
          type="button"
          className={s.railCollapse}
          data-testid="rail-collapse"
          aria-label="Collapse activity rail"
          aria-expanded={true}
          onClick={() => setOverride(false)}
        >
          <span aria-hidden="true">›</span>
        </button>
      </div>
      <StackPanel view={view} targeting={targeting} onInspect={onInspect} />
      {/*
       * The game log (issue #260) fills the rail's reserved dock: a readable, scrollable
       * history composed client-side from `view.log`, with clickable entity/player
       * references that ask the table to highlight their object.
       */}
      <GameLog view={view} onHighlight={onHighlight} highlightedId={highlightedId} />
    </div>
  );
}
