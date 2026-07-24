import { afterEach, vi } from 'vitest';
import { cleanup } from '@testing-library/react';
import { useGameStore } from '../store';

/**
 * Shared test support for the `Table` suites (split out of the former monolithic
 * `Table.test.tsx`, issue #433). These helpers are test-only and drive the real
 * store singleton — no production code depends on them.
 */

/**
 * The routing/decision tests drive the real store singleton (feeding it a lone
 * GameView, exactly the reconstruct-from-one-GameView seam) and spy on `choose`,
 * so we assert the id echoed back rather than any socket traffic.
 */
export function seed(json: string): ReturnType<typeof vi.fn> {
  const choose = vi.fn();
  useGameStore.getState().ingest(json);
  useGameStore.setState({ choose });
  return choose;
}

/**
 * Register the per-test cleanup shared by every Table suite: unmount the tree and
 * clear the store view so no state leaks across tests. Call once at the top level
 * of each Table test module.
 */
export function registerTableTestHooks(): void {
  afterEach(() => {
    cleanup();
    useGameStore.setState({ view: null });
  });
}
