/**
 * Whether the environment asks for reduced motion (`prefers-reduced-motion`). Drives
 * the summary-tile expand/collapse snap (issue #400). Read via a media query, absent
 * → false (SSR/older jsdom), and kept live so a mid-session OS change is honored.
 */
import { useEffect, useState } from 'react';

export function useReducedMotion(): boolean {
  const query = '(prefers-reduced-motion: reduce)';
  const read = (): boolean =>
    typeof window !== 'undefined' && typeof window.matchMedia === 'function'
      ? window.matchMedia(query).matches
      : false;
  const [reduced, setReduced] = useState(read);
  useEffect(() => {
    if (typeof window === 'undefined' || typeof window.matchMedia !== 'function') return;
    const mq = window.matchMedia(query);
    const onChange = (): void => setReduced(mq.matches);
    mq.addEventListener?.('change', onChange);
    return () => mq.removeEventListener?.('change', onChange);
  }, []);
  return reduced;
}
