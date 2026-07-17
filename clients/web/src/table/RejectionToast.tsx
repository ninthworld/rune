/**
 * The rejected-action toast (issue #265).
 *
 * When an in-game `choose_action` is rejected, the server re-sends the current
 * `GameView` flagged `action_rejected` (docs/protocol.md); the store turns that flag
 * into a monotonically increasing {@link GameStore.rejectionNonce}. This component
 * watches that counter and shows a brief, non-blaming "the game moved on" notice each
 * time it increments, auto-dismissing after a timeout.
 *
 * It is **ephemeral presentation only** — never load-bearing state. Per AGENTS.md the
 * whole table reconstructs from a single `GameView`; the toast holds no game state, is
 * not persisted, and a resync (which clears the flag, leaving the nonce unchanged) shows
 * nothing. It also never blocks input: the layer ignores pointer events entirely, so a
 * lingering toast can never swallow a click meant for the board or the action bar.
 */
import { useEffect, useRef, useState } from 'react';
import s from './chrome.module.css';

/** Default copy: informational ("the game moved on"), never blaming the player. */
const DEFAULT_MESSAGE = 'The game moved on — that action was no longer available.';
/** How long the toast stays up before auto-dismissing, in milliseconds. */
const DEFAULT_DURATION_MS = 4000;

export interface RejectionToastProps {
  /**
   * The store's rejection trigger counter. Each increment past the value seen at mount
   * fires the toast; the initial value is treated as baseline (a rejection that happened
   * before this component mounted is stale and shows nothing).
   */
  nonce: number;
  /** Override the displayed copy (defaults to the non-blaming "the game moved on" line). */
  message?: string;
  /** Override the auto-dismiss delay in ms (defaults to {@link DEFAULT_DURATION_MS}). */
  durationMs?: number;
}

export function RejectionToast({
  nonce,
  message = DEFAULT_MESSAGE,
  durationMs = DEFAULT_DURATION_MS,
}: RejectionToastProps) {
  const [visible, setVisible] = useState(false);
  // The last nonce we reacted to. Seeded with the mount value so only a *new* rejection
  // (an increment) ever triggers the toast — never the baseline.
  const seenNonce = useRef(nonce);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    // Only an increment fires the toast. A re-render with an unchanged nonce (a plain
    // resync that cleared the flag) leaves the toast alone.
    if (nonce === seenNonce.current) return;
    seenNonce.current = nonce;
    setVisible(true);
    // Reset the auto-dismiss window so back-to-back rejections keep the toast up for a
    // fresh full duration rather than dismissing on the first one's timer.
    if (timer.current !== null) clearTimeout(timer.current);
    timer.current = setTimeout(() => {
      setVisible(false);
      timer.current = null;
    }, durationMs);
  }, [nonce, durationMs]);

  // Clear any pending auto-dismiss timer on unmount so it never fires into a gone tree.
  useEffect(
    () => () => {
      if (timer.current !== null) clearTimeout(timer.current);
    },
    [],
  );

  if (!visible) return null;
  return (
    <div className={s.toastLayer} role="status" aria-live="polite">
      <div className={s.toast} data-testid="rejection-toast">
        {message}
      </div>
    </div>
  );
}
