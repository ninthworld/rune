/**
 * The measured viewport (width, height, pointer precision) the shell lays out
 * from, tracking the window so the whole table re-lays-out live on resize (the
 * layout itself stays a pure function — this only feeds it the live geometry).
 * Pointer precision is a capability, not a device (detected via a media query,
 * absent → `fine`), per ui-requirements §Input capability model.
 */
import { useEffect, useState } from 'react';
import type { Viewport } from '../layout';

function detectPointer(): Viewport['pointer'] {
  if (typeof window === 'undefined' || typeof window.matchMedia !== 'function') return 'fine';
  return window.matchMedia('(pointer: coarse)').matches ? 'coarse' : 'fine';
}

export function useViewport(): Required<Viewport> {
  const read = (): Required<Viewport> =>
    typeof window === 'undefined'
      ? { width: 1280, height: 800, pointer: 'fine' }
      : {
          width: window.innerWidth,
          height: window.innerHeight,
          pointer: detectPointer() ?? 'fine',
        };
  const [viewport, setViewport] = useState(read);
  useEffect(() => {
    if (typeof window === 'undefined') return;
    const onResize = (): void => setViewport(read());
    window.addEventListener('resize', onResize);
    return () => window.removeEventListener('resize', onResize);
  }, []);
  return viewport;
}
