import { afterEach, describe, expect, it } from 'vitest';
import { act, renderHook } from '@testing-library/react';
import type { GameLogEntry } from '../protocol';
import { useUnreadLog } from './useUnreadLog';

/** Build a window of log entries with the given sequence numbers. */
function log(...sequences: number[]): GameLogEntry[] {
  return sequences.map((sequence) => ({
    sequence,
    event: { type: 'hand_kept', player: 'p0' },
  }));
}

/** Set the document's visibility and fire the event the hook listens for. */
function setVisibility(state: 'visible' | 'hidden'): void {
  Object.defineProperty(document, 'visibilityState', {
    configurable: true,
    get: () => state,
  });
  act(() => {
    document.dispatchEvent(new Event('visibilitychange'));
  });
}

afterEach(() => setVisibility('visible'));

describe('useUnreadLog (issue #340)', () => {
  it('treats the entries carried by the first view as seen (fresh mount is clean)', () => {
    const { result } = renderHook(() => useUnreadLog(log(1, 2, 3)));
    expect(result.current.unreadCount).toBe(0);
    expect(result.current.isUnseen(3)).toBe(false);
  });

  it('does not mark entries unread while the tab stays visible (live tail)', () => {
    const { result, rerender } = renderHook(({ entries }) => useUnreadLog(entries), {
      initialProps: { entries: log(1, 2) },
    });
    // New entries arrive while visible — seen as they happen, so nothing is unread.
    rerender({ entries: log(1, 2, 3, 4) });
    expect(result.current.unreadCount).toBe(0);
  });

  it('marks entries that arrive while hidden as unread on return, and clears on view', () => {
    const { result, rerender } = renderHook(({ entries }) => useUnreadLog(entries), {
      initialProps: { entries: log(1, 2) },
    });

    setVisibility('hidden');
    rerender({ entries: log(1, 2, 3, 4) }); // two events arrive in the background

    setVisibility('visible'); // returning to the tab
    expect(result.current.unreadCount).toBe(2);
    expect(result.current.isUnseen(3)).toBe(true);
    expect(result.current.isUnseen(4)).toBe(true);
    expect(result.current.isUnseen(2)).toBe(false);

    act(() => result.current.markSeen()); // the reader views the log
    expect(result.current.unreadCount).toBe(0);
  });

  it('clamps the unread count to entries still in the bounded window', () => {
    const { result, rerender } = renderHook(({ entries }) => useUnreadLog(entries), {
      initialProps: { entries: log(1, 2) },
    });

    setVisibility('hidden');
    // Many events arrived, but the window only retains the most recent three; the
    // count must reflect what is displayable, never invent the entries that fell out.
    rerender({ entries: log(8, 9, 10) });
    setVisibility('visible');

    expect(result.current.unreadCount).toBe(3);
  });

  it('resumes the live tail after being marked seen', () => {
    const { result, rerender } = renderHook(({ entries }) => useUnreadLog(entries), {
      initialProps: { entries: log(1) },
    });
    setVisibility('hidden');
    rerender({ entries: log(1, 2) });
    setVisibility('visible');
    expect(result.current.unreadCount).toBe(1);

    act(() => result.current.markSeen());
    // Further activity while visible is seen live again.
    rerender({ entries: log(1, 2, 3) });
    expect(result.current.unreadCount).toBe(0);
  });
});
