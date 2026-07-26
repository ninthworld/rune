/**
 * The **presentation clock** (issue #594) — the one timer that walks the server's
 * ordered window of {@link PresentationMoment}s through
 * {@link ./presentationTrail}'s pure scheduler and publishes whichever moment is
 * on screen right now.
 *
 * All of the pacing lives in `presentationTrail.ts`, which holds no clock and no
 * React. This hook is the thin, impure half: it reads `GameView.presentation` off
 * each freshly presented view, drives a single `window.setTimeout` from the
 * scheduler's own reported wake time, and re-renders its consumer with the staged
 * caption. It is modelled on {@link useSessionMoments} for exactly that reason —
 * one timer, owned in one place, torn down on unmount.
 *
 * ## The four contracts this hook implements once for every consumer
 *
 * - **It never delays a view.** The store is untouched; `store.view` is applied
 *   the instant it arrives and every control is live from that frame. The trail
 *   paces a *caption* over a board that is already authoritative and already
 *   answerable (AGENTS.md hard rule; `presentation-budgets.md` §Performance).
 *   Nothing here returns a view, gates input, holds a prompt, or defers a store
 *   call. Losing the whole trail would cost the player no game information.
 * - **A discontinuity jumps to the present.** A bumped {@link GameStore.sessionEpoch}
 *   is the reconnect/resync/tab-restore signal (issue #493): everything queued
 *   before it describes a session that no longer exists, and the view being
 *   rendered is *already* the state those moments led to. So the backlog is
 *   dropped rather than replayed — the same choice `LivePlane` makes when it
 *   rebuilds rather than reconciles.
 * - **A decision hurries cosmetics; it does not erase them.** When a decision
 *   *arrives* — this seat gains something to answer, or an authoritative
 *   {@link GameView.action_deadline} clock starts — the backlog is
 *   {@link accelerate}d: low-value captions go, the causal chain survives at the
 *   floor dwell, and the whole thing is bounded by
 *   {@link PRESENTATION_DECISION_MS}. It is deliberately *not* dropped. The server
 *   settles a removal to rest with the caster holding priority, so the window
 *   carrying cast → resolved → died reaches the helpless seat one broadcast
 *   *before* that seat's own decision; discarding on the decision edge would leave
 *   the player with the bare board diff this contract exists to replace. Nothing
 *   is gated either way — the board and its actions are live from the frame they
 *   arrive on, so this bounds how long a timer ticks behind a caption, never how
 *   long a player waits to act.
 * - **A late tick catches up, it does not replay.** A backgrounded or throttled
 *   tab wakes long after its deadline. The trail compresses what it slept
 *   through, and if it was away longer than a whole backlog it fast-forwards
 *   outright, so a restored tab shows the present rather than narrating minutes of
 *   history in real time.
 *
 * ## What it deliberately does not do
 *
 * It does not read the store, so a consumer's own `sessionEpoch` subscription is
 * the single source of that signal and the hook stays testable without a socket.
 * It does not sort, gap-fill, or de-duplicate a window — {@link enqueue}'s
 * watermark makes a duplicate delivery free, and a gap in a receiver's id stream
 * is normal (the window is bounded, and another seat's `phases_skipped` is
 * filtered out of this one), never a lost message to wait for.
 */
import { useCallback, useEffect, useRef, useState } from 'react';
import type { GameView } from '../../protocol';
import {
  accelerate,
  advance,
  compress,
  createPresentationTrail,
  enqueue,
  fastForward,
  isTrailIdle,
  withReducedMotion,
  PRESENTATION_BACKLOG_MS,
  type PresentationTrail,
  type StagedMoment,
} from './presentationTrail';

/** What the clock needs beyond the view itself. */
export interface PresentationTrailOptions {
  /**
   * Whether the player asked for reduced motion (issue #505). It suppresses
   * *travel only* — {@link StagedMoment.travel} — and never the dwell: a caption
   * has no motion to remove, and a reader who asked for less movement did not ask
   * to read faster. The staged sequence is identical either way.
   */
  reducedMotion?: boolean;
  /**
   * The transport generation of the connection the view arrived on
   * (`GameStore.sessionEpoch`). Any change is a discontinuity and fast-forwards
   * the trail. Passed in rather than read from the store so this hook has one
   * input surface and no store coupling.
   */
  sessionEpoch?: number;
}

/**
 * Stage the server's presentation window, one moment at a time.
 *
 * @param view the latest complete `GameView`, or `null` before the first one
 * @returns the moment currently on screen, or `null` when the trail is quiet
 */
export function usePresentationTrail(
  view: GameView | null,
  options: PresentationTrailOptions = {},
): StagedMoment | null {
  const { reducedMotion = false, sessionEpoch = 0 } = options;
  const [staged, setStaged] = useState<StagedMoment | null>(null);
  const trailRef = useRef<PresentationTrail>(createPresentationTrail({ reducedMotion }));
  const timerRef = useRef<number | null>(null);
  // The clock reading the armed timer was asked for, so a tick can tell how late
  // it actually ran. `null` while no timer is armed.
  const wakeRef = useRef<number | null>(null);
  const epochRef = useRef(sessionEpoch);
  // Whether the previous view already had this seat answering something. The
  // *edge* is the signal, not the state: an ordinary run of frames where the seat
  // holds priority throughout must not fast-forward on every one of them, or
  // nothing would ever play.
  const decidingRef = useRef(false);
  const clockedRef = useRef(false);
  // The pump, reachable from inside the timer it arms. A plain reference would be
  // a use-before-definition cycle.
  const pumpRef = useRef<(now: number) => void>(() => {});

  const clearTimer = useCallback((): void => {
    if (timerRef.current !== null) window.clearTimeout(timerRef.current);
    timerRef.current = null;
    wakeRef.current = null;
  }, []);

  /**
   * Publish the trail as of `now` and arm the single timer for its next wake.
   *
   * {@link advance} is idempotent for a given `now`, so calling this from the
   * ingest effect and from the timer cannot double-advance the trail.
   */
  const pump = useCallback(
    (now: number): void => {
      clearTimer();
      const result = advance(trailRef.current, now);
      trailRef.current = result.trail;
      setStaged(result.staged);
      if (result.wakeAt === null) return;
      wakeRef.current = result.wakeAt;
      timerRef.current = window.setTimeout(
        () => {
          const at = Date.now();
          // How far past its deadline this tick actually ran. A throttled or
          // backgrounded tab can be minutes late; a healthy frame is a millisecond
          // or two, and `compress` is a no-op under its cap, so the ordinary path
          // pays nothing for this.
          const lateBy = wakeRef.current === null ? 0 : at - wakeRef.current;
          trailRef.current =
            lateBy > PRESENTATION_BACKLOG_MS
              ? fastForward(trailRef.current)
              : compress(trailRef.current);
          pumpRef.current(at);
        },
        Math.max(0, result.wakeAt - now),
      );
    },
    [clearTimer],
  );
  pumpRef.current = pump;

  // A mid-match motion-preference change reaches what has not played yet; what is
  // already on screen keeps the treatment it started with rather than switching
  // under the eye. Never restages, so the sequence is untouched.
  useEffect(() => {
    trailRef.current = withReducedMotion(trailRef.current, reducedMotion);
  }, [reducedMotion]);

  useEffect(() => {
    if (view === null) return;
    const discontinuity = sessionEpoch !== epochRef.current;
    epochRef.current = sessionEpoch;
    const clocked = view.action_deadline !== undefined;
    const deciding = view.valid_actions.length > 0 || clocked;
    // The rising edge only: a decision this seat did not have a frame ago, or a
    // deadline clock that has just started running. `action_deadline` counts *down*
    // across frames, so its value changing is not an arrival and must not retrigger.
    const decisionArrived = (deciding && !decidingRef.current) || (clocked && !clockedRef.current);
    decidingRef.current = deciding;
    clockedRef.current = clocked;

    const before = trailRef.current;
    let trail = before;
    // A discontinuity discards; a decision only hurries. The moments queued behind
    // a decision are the causal explanation of the board that decision is being
    // asked about — dropping them is the one failure mode issue #594 exists to
    // prevent, and the server routinely delivers the chain and the decision on
    // consecutive broadcasts.
    if (discontinuity) trail = fastForward(trail);
    else if (decisionArrived) trail = accelerate(trail);
    trail = compress(enqueue(trail, view.presentation ?? []));
    // Reference-identical means nothing was admitted and nothing was dropped: a
    // redelivery of a window already seen, which must not restage anything or
    // disturb the caption the reader is mid-way through.
    //
    // It may only be skipped, though, while the clock this hook owns is still
    // running — or has nothing left to run for. React's development strict-effects
    // pass mounts the effect, tears it down (cancelling the armed timer), then
    // mounts it again with the *same* view: the second pass admits nothing, so an
    // unconditional early return here would leave the opening settle's first
    // caption frozen on screen with no timer behind it until some later view
    // happened to arrive. Re-pumping instead is free — {@link advance} is
    // idempotent for a given `now`, so the staged moment and its remaining dwell
    // are exactly what they were and only the cancelled timer comes back. Outside
    // that teardown the condition never fires: a non-idle trail always reports a
    // wake time, so it always has a timer armed.
    if (trail === before && (timerRef.current !== null || isTrailIdle(trail))) return;
    trailRef.current = trail;
    // The pump is reached through a ref, so a freshly presented view — and the
    // transport generation it arrived on — are the only ingest triggers.
    pumpRef.current(Date.now());
  }, [view, sessionEpoch]);

  useEffect(() => () => clearTimer(), [clearTimer]);

  return staged;
}
