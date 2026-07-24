/**
 * Whether motion should be reduced, composing the OS `prefers-reduced-motion`
 * query with the player's device-local motion preference (issue #505). The OS
 * query is read live (a mid-session OS change is honored) and absent → false
 * (SSR / older jsdom).
 *
 * Composition (`resolveReducedMotion`): **OS-on OR user-on ⇒ reduced**. The OS
 * request is authoritative — an accessibility setting is never overridden by an
 * in-app preference — so `full` prefers full motion only when the OS allows it,
 * still yielding to an OS "reduce". Called with no argument it defaults to
 * `system`, so existing OS-only callers (e.g. the #400 summary-tile snap) are
 * unchanged.
 */
import { useEffect, useState } from 'react';
import { resolveReducedMotion, type MotionPreference } from '../settings/presentationSettings';

export function useReducedMotion(motion: MotionPreference = 'system'): boolean {
  const query = '(prefers-reduced-motion: reduce)';
  const read = (): boolean =>
    typeof window !== 'undefined' && typeof window.matchMedia === 'function'
      ? window.matchMedia(query).matches
      : false;
  const [osReduced, setOsReduced] = useState(read);
  useEffect(() => {
    if (typeof window === 'undefined' || typeof window.matchMedia !== 'function') return;
    const mq = window.matchMedia(query);
    const onChange = (): void => setOsReduced(mq.matches);
    mq.addEventListener?.('change', onChange);
    return () => mq.removeEventListener?.('change', onChange);
  }, []);
  return resolveReducedMotion(osReduced, motion);
}
