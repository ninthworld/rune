/**
 * Reconnect / resync behavior of the production match surface (issue #493):
 * a transport-generation discontinuity rebuilds the complete board from the one
 * latest view, clears pre-disconnect ephemeral UI, and flashes the single
 * "you are here" phase-pill cue. Store-driven, exercising the real
 * reconstruct-from-one-GameView seam.
 */
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { SAMPLE_GAME_VIEW_JSON } from '../../game-view.fixture';
import { useGameStore } from '../../store';
import { registerTableTestHooks, seed } from '../table-test-support';
import { LiveMatchTable } from './LiveMatchTable';

vi.mock('../EffectsSurface', () => ({
  EffectsSurface: () => <div data-testid="effects-surface" aria-hidden="true" />,
}));
vi.mock('../effects', () => ({
  EffectsLayer: class {
    setPersistent(): void {}
    replaceTransients(): void {}
    trackMotion(): void {}
  },
}));

registerTableTestHooks();

/** Bump the transport generation and deliver a fresh full view — a reconnect. */
function reconnect(): void {
  act(() => {
    useGameStore.setState((state) => ({ sessionEpoch: state.sessionEpoch + 1 }));
    useGameStore.getState().ingest(SAMPLE_GAME_VIEW_JSON);
  });
}

describe('LiveMatchTable reconnect / resync', () => {
  beforeEach(() => {
    vi.spyOn(window, 'requestAnimationFrame').mockReturnValue(1);
    vi.spyOn(window, 'cancelAnimationFrame').mockImplementation(() => undefined);
  });

  it('rebuilds the complete board and flashes the "you are here" cue on reconnect', async () => {
    act(() => {
      useGameStore.setState({ sessionEpoch: 1 });
    });
    seed(SAMPLE_GAME_VIEW_JSON);
    render(<LiveMatchTable />);
    const shell = screen.getByTestId('live-match-table');
    expect(shell.getAttribute('data-orienting')).toBeNull();

    reconnect();

    // The complete latest board is present from the one reconnect view…
    expect(shell.getAttribute('data-orienting')).toBe('true');
    expect(document.querySelectorAll('[data-slot="region"]').length).toBeGreaterThan(0);
    expect(screen.getByTestId('live-2-5d-plane')).toBeTruthy();

    // …input is ready immediately, never gated on the orientation cue: a hit
    // target is present and enabled the moment the rebuilt scene is exposed.
    const handCard = screen.getByTestId<HTMLButtonElement>('live-hand-card-c1');
    expect(handCard.disabled).toBe(false);

    // …and the cue is non-blocking: it retires itself.
    await waitFor(() => expect(shell.getAttribute('data-orienting')).toBeNull());
  });

  it('does not let a pre-disconnect inspect overlay survive the rebuild', () => {
    act(() => {
      useGameStore.setState({ sessionEpoch: 1 });
    });
    seed(SAMPLE_GAME_VIEW_JSON);
    render(<LiveMatchTable />);

    // Open an ephemeral inspect surface before the disconnect.
    fireEvent.click(screen.getByTestId('live-hand-card-c1'));
    expect(screen.getByTestId('card-inspect')).toBeTruthy();

    reconnect();

    // The rebuilt scene exposes no stale overlay.
    expect(screen.queryByTestId('card-inspect')).toBeNull();
  });
});
