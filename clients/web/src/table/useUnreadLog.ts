/**
 * Unread game-log activity after returning to the tab (issue #340,
 * ui-requirements §Comprehension: "visible recent changes and unread activity after
 * returning to the tab").
 *
 * With server-owned priority automation (#264) the game can advance several steps
 * while the tab is backgrounded, so a returning player needs a signal that something
 * happened and where to start reading. This hook derives that signal from the log
 * window alone, driven by document visibility — never a timer.
 *
 * The state is **ephemeral presentation, never load-bearing** (AGENTS.md hard rule):
 * the only thing tracked is the highest log `sequence` the reader has seen. A remount
 * or reconnect reinitializes it from the first view's window, so the game is still
 * fully reconstructed from one `GameView` — losing the marker loses no game
 * information, and a fresh mount starts clean (the carried entries count as seen, so
 * only entries arriving *after* mount can become unread — no false "everything
 * unread" alarm).
 *
 * Model:
 * - `seenSeq` starts at the newest sequence present at mount.
 * - While the tab is **visible** and nothing is pending review, `seenSeq` tracks the
 *   newest entry as it arrives (a live tail — entries seen as they happen).
 * - When the tab is **hidden**, `seenSeq` stops advancing, so entries arriving in the
 *   background accumulate as unread.
 * - On returning to **visible** with unseen entries, review is *pending*: the live
 *   tail pauses so the unread indicator stays up until the reader actually views the
 *   log ({@link markSeen}) — clicking the rail affordance or scrolling to the newest.
 *
 * Unread is counted over the entries *present in the window*, so counts clamp to what
 * is displayable: if entries fell out of the bounded window between hide and show, the
 * count never invents phantom history.
 */
import { useCallback, useEffect, useState } from 'react';
import type { GameLogEntry } from '../protocol';

/** Whether the document is currently visible; `true` outside a browser (SSR/tests
 * without a document) so nothing depends on a missing API. */
function isVisible(): boolean {
  return typeof document === 'undefined' || document.visibilityState === 'visible';
}

/** The newest (highest) sequence in the window, or `0` for an empty window. */
function newestSequence(entries: GameLogEntry[]): number {
  return entries.length > 0 ? entries[entries.length - 1].sequence : 0;
}

export interface UnreadLog {
  /** How many entries currently in the window are unseen — clamped to the displayable
   * window, so it never invents entries that fell out of it. `0` in the live tail. */
  unreadCount: number;
  /** Whether a specific entry's `sequence` is unseen, for the per-entry distinction. */
  isUnseen: (sequence: number) => boolean;
  /** Mark everything currently in the window as seen (the reader viewed the log):
   * clears the indicator and resumes the live tail. */
  markSeen: () => void;
}

export function useUnreadLog(entries: GameLogEntry[]): UnreadLog {
  const newest = newestSequence(entries);
  // The highest sequence the reader has seen; initialized to the mount-time window so
  // the entries a fresh mount carries are treated as already seen.
  const [seenSeq, setSeenSeq] = useState(newest);
  // True once the tab has returned to the foreground with unseen entries waiting: the
  // live tail pauses so the indicator persists until the reader views the log.
  const [pendingReview, setPendingReview] = useState(false);

  // React to the tab going to the background and back. Returning with entries beyond
  // what was seen opens a pending review; the live-tail effect then holds the marker.
  useEffect(() => {
    const onVisibility = (): void => {
      if (isVisible() && newestSequence(entries) > seenSeq) {
        setPendingReview(true);
      }
    };
    document.addEventListener('visibilitychange', onVisibility);
    window.addEventListener('focus', onVisibility);
    return () => {
      document.removeEventListener('visibilitychange', onVisibility);
      window.removeEventListener('focus', onVisibility);
    };
  }, [entries, seenSeq]);

  // Live tail: while visible and not reviewing a backlog, keep `seenSeq` at the newest
  // entry so activity watched in real time never registers as unread.
  useEffect(() => {
    if (!pendingReview && isVisible() && newest > seenSeq) {
      setSeenSeq(newest);
    }
  }, [newest, pendingReview, seenSeq]);

  const markSeen = useCallback(() => {
    setPendingReview(false);
    setSeenSeq((prev) => (newest > prev ? newest : prev));
  }, [newest]);

  const unreadCount = entries.reduce((n, entry) => (entry.sequence > seenSeq ? n + 1 : n), 0);
  const isUnseen = useCallback((sequence: number) => sequence > seenSeq, [seenSeq]);

  return { unreadCount, isUnseen, markSeen };
}
