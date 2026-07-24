/**
 * The session-moment clock (issue #509) — the one timer that stages the §8
 * moments the match shell owns: the **game start** assembly, the **reconnect**
 * "you are here" acknowledgment, and the **return to lobby** recede.
 *
 * The moments themselves (names, budgets, skippability, hue families) are the
 * pure vocabulary of {@link ./sessionMoments}; this hook only runs their clock
 * and publishes which one is currently staging, so the shell can wear it as a
 * data attribute and let CSS do the rest.
 *
 * The binding contracts, implemented here once for every consumer:
 *
 * - **Never gates input.** The staged attribute drives opacity/transform only;
 *   every control is mounted, enabled, and hit-testable throughout. Nothing
 *   here defers a store call except the exit, which the player asked for.
 * - **Interruptible.** A newer presented view retargets the moment: the running
 *   clock is dropped and the new one starts clean, so no pre-reconnect staging
 *   can survive into a rebuilt scene.
 * - **Skippable where §8 marks it.** A skippable moment ends on the first
 *   deliberate input (pointer or key) instead of running its full window.
 * - **Reduced motion snaps.** {@link momentDurationMs} returns 0, so no moment
 *   is ever staged and the exit runs synchronously — zero staged frames, zero
 *   state difference.
 */
import { useCallback, useEffect, useRef, useState } from 'react';
import type { PresentationMode } from './presentationMode';
import { entryMoment, isSkippable, momentDurationMs, type SessionMoment } from './sessionMoments';

/** What the shell needs to stage its session moments. */
export interface SessionMomentsApi {
  /** The moment currently staging, or `null` when the scene is settled. */
  moment: SessionMoment | null;
  /** Note the presentation mode of a freshly presented view (from `LivePlane`). */
  notePresentationMode: (mode: PresentationMode) => void;
  /**
   * Leave the finished game: stage the ≤ 400 ms recede, then run the store
   * transition. Idempotent while the recede is in flight, and immediate under
   * reduced motion (the §8 RM form for this row is a cut).
   */
  leave: () => void;
}

/**
 * Stage the shell-owned session moments.
 *
 * @param reducedMotion collapse every moment to its end state
 * @param onLeave the store transition out of a finished game (issue #452);
 *   omitted where there is no session to leave
 */
export function useSessionMoments(reducedMotion: boolean, onLeave?: () => void): SessionMomentsApi {
  const [moment, setMoment] = useState<SessionMoment | null>(null);
  const timerRef = useRef<number | null>(null);
  const detachRef = useRef<(() => void) | null>(null);
  // Once the exit is committed the shell is on its way out; later view
  // transitions must not restage an entry moment over the recede.
  const leavingRef = useRef(false);
  const reducedRef = useRef(reducedMotion);
  const onLeaveRef = useRef(onLeave);
  reducedRef.current = reducedMotion;
  onLeaveRef.current = onLeave;

  const clearClock = useCallback((): void => {
    if (timerRef.current !== null) window.clearTimeout(timerRef.current);
    timerRef.current = null;
    detachRef.current?.();
    detachRef.current = null;
  }, []);

  const stage = useCallback(
    (next: SessionMoment, done?: () => void): void => {
      clearClock();
      const durationMs = momentDurationMs(next, reducedRef.current);
      if (durationMs === 0) {
        // Reduced motion: already at the end state, so there is no staged frame.
        setMoment(null);
        done?.();
        return;
      }
      setMoment(next);
      const settle = (): void => {
        clearClock();
        // The completion runs before the staged flag drops so an exit hands off
        // straight from the receded scene instead of flashing it back in.
        done?.();
        setMoment(null);
      };
      timerRef.current = window.setTimeout(settle, durationMs);
      if (!isSkippable(next)) return;
      const onInput = (): void => settle();
      window.addEventListener('pointerdown', onInput);
      window.addEventListener('keydown', onInput);
      detachRef.current = () => {
        window.removeEventListener('pointerdown', onInput);
        window.removeEventListener('keydown', onInput);
      };
    },
    [clearClock],
  );

  const notePresentationMode = useCallback(
    (mode: PresentationMode): void => {
      if (leavingRef.current) return;
      const next = entryMoment(mode);
      // Ordinary play stages nothing and disturbs nothing: an entry moment is a
      // fixed, self-retiring window that a later frame has no reason to cut
      // short. Interruption of the *scene* is `LivePlane`'s job and already
      // instantaneous — a newer view reconciles (or rebuilds) underneath the
      // ramp, so what the player sees is always the latest state. A newer entry
      // moment does replace the running one: a reconnect rebuild mid-assembly
      // drops the assembly and shows the "you are here" cue instead.
      if (next !== null) stage(next);
    },
    [stage],
  );

  const leave = useCallback((): void => {
    if (leavingRef.current) return;
    leavingRef.current = true;
    stage('return-to-lobby', () => onLeaveRef.current?.());
  }, [stage]);

  useEffect(() => clearClock, [clearClock]);

  return { moment, notePresentationMode, leave };
}
