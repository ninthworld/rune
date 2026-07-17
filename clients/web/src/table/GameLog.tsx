/**
 * The game log panel (React DOM, ADR 0003 — the log is prose a user reads, so it is
 * DOM chrome, not the Pixi canvas). Issue #260 / ui-requirements §Comprehension.
 *
 * Renders the bounded {@link GameView.log} window so a newcomer can follow what
 * happened: oldest at the top, newest at the bottom, the whole history scrollable, and
 * the view auto-pinned to the bottom unless the reader has scrolled up to look back.
 * Every line is composed *client-side* from a structured event by {@link describeEvent}
 * — the server sends no prose — and repetitive turn/phase runs collapse behind one
 * expandable summary ({@link groupEntries}) so the signal is not lost in step spam.
 *
 * Entity and player references are interactive but PURELY presentational (issue #260,
 * AGENTS.md hard rule): clicking one calls {@link Props.onHighlight} with the opaque id
 * so the table can ring the referenced permanent or highlight the player's tile. No
 * legality, cost, or rules is derived here — a click only asks for a highlight, and a
 * dead reference (the object has left the battlefield) simply highlights nothing.
 *
 * Pure render of the latest view: the log content comes entirely from `view.log`, so a
 * fresh mount from one mid-game `GameView` shows exactly what a client that watched the
 * whole game shows (within the carried window). The only local state is ephemeral
 * presentation — the auto-scroll pin and a step group's expanded/collapsed toggle —
 * none of it load-bearing across messages (the reconnect/replay invariant).
 */
import { useEffect, useRef, useState } from 'react';
import type { EntityId, GameLogEntry, GameView } from '../protocol';
import { cx } from '../chrome/cx';
import { describeEvent, groupEntries, isRef, type LogGroup, type LogSegment } from './gameLog';
import s from './chrome.module.css';

interface Props {
  /** The latest view; the panel renders exactly its `log` window and names players
   * from its `player_names` map. */
  view: GameView;
  /** Presentationally highlight the clicked reference's object (permanent on the
   * battlefield, or a player's tile). Omitted in read-only contexts. */
  onHighlight?: (id: EntityId) => void;
  /** The id currently highlighted, if any — marks its references as pressed. */
  highlightedId?: EntityId | null;
}

/** How close to the bottom (px) still counts as "pinned", so sub-pixel rounding or a
 * trailing margin never unpins the auto-scroll. */
const PIN_THRESHOLD = 24;

/** Render one composed line's segments: literal text as-is, each reference as a
 * pointer/keyboard/touch-reachable button that asks the table to highlight its object. */
function renderSegments(
  segments: LogSegment[],
  onHighlight: Props['onHighlight'],
  highlightedId: Props['highlightedId'],
): React.ReactNode {
  return segments.map((segment, i) => {
    if (!isRef(segment)) return <span key={i}>{segment}</span>;
    const pressed = highlightedId != null && highlightedId === segment.id;
    return (
      <button
        key={i}
        type="button"
        className={cx(s.logRef, pressed && s.logRefActive)}
        data-testid={`log-ref-${segment.id}`}
        data-entity={segment.id}
        aria-pressed={pressed}
        aria-label={`Highlight ${segment.name}`}
        onClick={() => onHighlight?.(segment.id)}
      >
        {segment.name}
      </button>
    );
  });
}

/** A single log line for one entry. */
function EntryLine({
  entry,
  view,
  onHighlight,
  highlightedId,
}: {
  entry: GameLogEntry;
  view: GameView;
  onHighlight: Props['onHighlight'];
  highlightedId: Props['highlightedId'];
}) {
  return (
    <li className={s.logEntry} data-testid={`log-entry-${entry.sequence}`}>
      {renderSegments(describeEvent(entry.event, view), onHighlight, highlightedId)}
    </li>
  );
}

/**
 * A collapsed run of consecutive step changes. Collapsed (the default) shows only the
 * most recent step plus a toggle that reveals the earlier ones; expanded lists them all.
 * The toggle is ephemeral presentation — a fresh mount defaults to collapsed, so nothing
 * here is load-bearing across messages.
 */
function StepsGroup({
  entries,
  view,
  onHighlight,
  highlightedId,
}: {
  entries: GameLogEntry[];
  view: GameView;
  onHighlight: Props['onHighlight'];
  highlightedId: Props['highlightedId'];
}) {
  const [expanded, setExpanded] = useState(false);
  const earlier = entries.length - 1;
  const shown = expanded ? entries : [entries[entries.length - 1]];
  return (
    <li className={s.logSteps} data-testid="log-steps">
      <button
        type="button"
        className={s.logStepsToggle}
        data-testid="log-steps-toggle"
        aria-expanded={expanded}
        onClick={() => setExpanded((open) => !open)}
      >
        {expanded ? 'Hide earlier steps' : `+${earlier} earlier step${earlier === 1 ? '' : 's'}`}
      </button>
      <ol className={s.logStepsList}>
        {shown.map((entry) => (
          <li
            key={entry.sequence}
            className={cx(s.logEntry, s.logStepEntry)}
            data-testid={`log-entry-${entry.sequence}`}
          >
            {renderSegments(describeEvent(entry.event, view), onHighlight, highlightedId)}
          </li>
        ))}
      </ol>
    </li>
  );
}

export function GameLog({ view, onHighlight, highlightedId }: Props) {
  const entries = view.log ?? [];
  const groups: LogGroup[] = groupEntries(entries);

  // Auto-scroll: keep the newest entry in view unless the reader has scrolled up.
  // `pinned` is ephemeral presentation, tracked in a ref so it never triggers a
  // re-render; a fresh mount starts pinned to the bottom (the default), so the panel
  // is reconstructable from one view.
  const scrollRef = useRef<HTMLOListElement>(null);
  const pinnedRef = useRef(true);
  const lastSeq = entries.length > 0 ? entries[entries.length - 1].sequence : -1;

  const onScroll = (): void => {
    const el = scrollRef.current;
    if (!el) return;
    pinnedRef.current = el.scrollHeight - el.clientHeight - el.scrollTop <= PIN_THRESHOLD;
  };

  useEffect(() => {
    const el = scrollRef.current;
    if (!el || !pinnedRef.current) return;
    // Jump to the bottom on new content while pinned. Reduced-motion is honored
    // globally (base.css neutralizes scroll-behavior), so this stays a plain jump.
    el.scrollTop = el.scrollHeight;
  }, [lastSeq, entries.length]);

  return (
    <section className={s.gameLog} data-testid="game-log" aria-label="Game log">
      <h3 className={s.gameLogTitle}>Log</h3>
      {entries.length === 0 ? (
        <p className={s.gameLogEmpty} data-testid="game-log-empty">
          No game events yet.
        </p>
      ) : (
        <ol
          ref={scrollRef}
          className={s.gameLogList}
          data-testid="game-log-list"
          onScroll={onScroll}
          aria-live="polite"
        >
          {groups.map((group) =>
            group.kind === 'steps' ? (
              <StepsGroup
                key={`steps-${group.entries[0].sequence}`}
                entries={group.entries}
                view={view}
                onHighlight={onHighlight}
                highlightedId={highlightedId}
              />
            ) : (
              <EntryLine
                key={group.entry.sequence}
                entry={group.entry}
                view={view}
                onHighlight={onHighlight}
                highlightedId={highlightedId}
              />
            ),
          )}
        </ol>
      )}
    </section>
  );
}
