/**
 * The live decision countdown (issue #263), shared by the staged decision surfaces
 * (issue #298): it rides the anchored prompt overlay while a decision is being
 * resolved, and the action tray during a bare priority window. Extracted from the
 * prompt banner so both stagings render the identical clock (same testid + low-time
 * treatment) without duplicating the tick logic.
 *
 * Seeds from the server-sent seconds remaining ({@link GameView.action_deadline})
 * and ticks down locally once per second; a fresh view re-seeds it (the server
 * re-sends the real remaining time, so nothing here is load-bearing across messages
 * — a reconnect shows the right value). Enters a warning state under
 * {@link LOW_TIME_SECONDS}. The server, not the client, enforces the deadline —
 * this is display only.
 */
import { useEffect, useState } from 'react';
import s from './chrome.module.css';
import { cx } from '../chrome/cx';

/** Below this many seconds the countdown enters its low-time warning state. */
const LOW_TIME_SECONDS = 10;

export function DeadlineCountdown({ seconds }: { seconds: number }) {
  const [remaining, setRemaining] = useState(seconds);
  useEffect(() => {
    setRemaining(seconds);
    const id = setInterval(() => setRemaining((value) => Math.max(0, value - 1)), 1000);
    return () => clearInterval(id);
  }, [seconds]);
  const display = Math.max(0, Math.ceil(remaining));
  const low = display <= LOW_TIME_SECONDS;
  return (
    <span
      data-testid="deadline-countdown"
      data-low={low || undefined}
      className={cx(s.deadlineCountdown, low && s.deadlineCountdownLow)}
    >
      {' '}
      — {display}s
    </span>
  );
}
