/**
 * The stack & activity rail (ADR 0023; React DOM, issues #299/#260).
 *
 * On the full composition the rail is a **fixed carved column** on the right
 * edge, owning two permanent panels: the STACK on top (adjacent to the boards it
 * resolves into) and the ACTIVITY log below. Chrome never disappears and
 * reflows: an empty stack shows a designed quiet state instead of vanishing
 * (blueprint §Screen anatomy — the shipped collapse/auto-expand behavior is
 * retired with the floating shell).
 *
 * On the compact composition the same content renders as a **sheet** opened from
 * the top bar's stack/log chips — the only layer permitted to cover the shell,
 * viewport-clamped and dismissible.
 *
 * The stack itself is rendered by {@link StackPanel} unchanged: resolution order
 * top-first, the "resolves next" emphasis, controller/source meta, inspect
 * handles, and targeting-mode pickability all come from it.
 */
import type { EntityId, GameView } from '../protocol';
import { GameLog } from './GameLog';
import { StackPanel } from './StackPanel';
import { useUnreadLog } from './useUnreadLog';
import s from './chrome.module.css';

/** The active target slot's stack-object candidates plus the pick handler, forwarded
 * verbatim to {@link StackPanel} so a stack object stays pickable in targeting mode. */
interface TargetingStack {
  candidates: EntityId[];
  onPick: (id: EntityId) => void;
}

interface Props {
  /** The latest view; the rail renders exactly its `stack` and `log`. */
  view: GameView;
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

export function Rail({ view, targeting, onInspect, onHighlight, highlightedId }: Props) {
  // Unread game-log activity after returning to the tab (issue #340).
  const { unreadCount, isUnseen, markSeen } = useUnreadLog(view.log ?? []);
  const count = view.stack.length;

  return (
    <div className={s.rail} data-testid="rail" data-expanded="true">
      {/* The stack panel: fixed home at the rail's top. A populated stack lists its
          objects; an empty one shows the designed quiet state — the chrome never
          disappears, so the stack is always findable in the same place. */}
      <section className={s.railSection} data-testid="rail-stack">
        {count > 0 ? (
          <StackPanel view={view} targeting={targeting} onInspect={onInspect} />
        ) : (
          <>
            <h2 className={s.stackTitle}>Stack</h2>
            <p className={s.stackQuiet} data-testid="stack-quiet">
              Empty — spells and abilities appear here.
            </p>
          </>
        )}
      </section>
      {/* The activity log (issue #260): fills the rail's remaining height, scrolls
          internally, and carries the unread marker (issue #340). */}
      <section className={s.railSection} data-testid="rail-activity">
        <GameLog
          view={view}
          onHighlight={onHighlight}
          highlightedId={highlightedId}
          isUnseen={isUnseen}
          unreadCount={unreadCount}
          onSeen={markSeen}
        />
      </section>
    </div>
  );
}
