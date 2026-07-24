import { afterEach, describe, expect, it, vi } from 'vitest';
import { renderHook } from '@testing-library/react';
import { useReducedMotion } from './useReducedMotion';
import type { MotionPreference } from '../settings/presentationSettings';

/** Install a `matchMedia` stub reporting the given OS reduced-motion state. */
function stubMatchMedia(osReduced: boolean): void {
  vi.stubGlobal('matchMedia', (query: string) => ({
    matches: query.includes('reduce') ? osReduced : false,
    media: query,
    addEventListener: () => {},
    removeEventListener: () => {},
  }));
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('useReducedMotion (OS × user override matrix)', () => {
  const cases: { motion: MotionPreference; os: boolean; expected: boolean }[] = [
    { motion: 'system', os: false, expected: false },
    { motion: 'system', os: true, expected: true },
    { motion: 'reduced', os: false, expected: true },
    { motion: 'reduced', os: true, expected: true },
    { motion: 'full', os: false, expected: false },
    // OS reduced-motion is authoritative — `full` cannot override an accessibility setting.
    { motion: 'full', os: true, expected: true },
  ];

  it.each(cases)('motion=$motion, OS reduce=$os ⇒ $expected', ({ motion, os, expected }) => {
    stubMatchMedia(os);
    const { result } = renderHook(() => useReducedMotion(motion));
    expect(result.current).toBe(expected);
  });

  it('defaults to following the OS when called with no preference', () => {
    stubMatchMedia(true);
    const { result } = renderHook(() => useReducedMotion());
    expect(result.current).toBe(true);
  });
});
