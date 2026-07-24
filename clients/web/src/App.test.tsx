import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { App } from './App';
import { useGameStore, type SocketFactory } from './store';
import { GAME_OVER_LOSS_JSON, SAMPLE_GAME_VIEW_JSON } from './game-view.fixture';
import { LOBBY_ROOMLESS_JSON } from './lobby-view.fixture';

// The in-game screen is the 2.5D match table (#494); mock its passive Pixi
// effects layer so it mounts headless, exactly as the live-table suites do.
vi.mock('./table/EffectsSurface', () => ({
  EffectsSurface: () => <div data-testid="effects-surface" aria-hidden="true" />,
}));
vi.mock('./table/effects', () => ({
  EffectsLayer: class {
    setPersistent(): void {}
    replaceTransients(): void {}
    trackMotion(): void {}
  },
}));

/**
 * A manually-driven stand-in for the browser `WebSocket` (same shape store.test.ts
 * uses), so the App's state transitions are exercised end-to-end without real I/O.
 */
class FakeSocket {
  onopen: ((event: unknown) => void) | null = null;
  onmessage: ((event: { data: unknown }) => void) | null = null;
  onclose: ((event: unknown) => void) | null = null;
  onerror: ((event: unknown) => void) | null = null;
  closed = false;

  send(): void {}
  close(): void {
    this.closed = true;
    this.onclose?.({});
  }
  emitOpen(): void {
    this.onopen?.({});
  }
  emitMessage(data: string): void {
    this.onmessage?.({ data });
  }
  drop(): void {
    this.onclose?.({});
  }
}

function recordingFactory(): { factory: SocketFactory; sockets: FakeSocket[] } {
  const sockets: FakeSocket[] = [];
  const factory: SocketFactory = () => {
    const s = new FakeSocket();
    sockets.push(s);
    return s as unknown as WebSocket;
  };
  return { factory, sockets };
}

/** Open a connection through the real store with an injected fake socket. */
function connectWith(factory: SocketFactory): void {
  act(() =>
    useGameStore.getState().connect('ws://test', { createSocket: factory, autoReconnect: false }),
  );
}

beforeEach(() => {
  vi.spyOn(window, 'requestAnimationFrame').mockReturnValue(1);
  vi.spyOn(window, 'cancelAnimationFrame').mockImplementation(() => undefined);
});

afterEach(() => {
  cleanup();
  act(() => useGameStore.getState().disconnect());
  useGameStore.setState({ status: 'idle', view: null, lobby: null, lobbyError: null });
  vi.restoreAllMocks();
});

describe('App connection gating (issues #103, #114)', () => {
  it('cold-starts on the front-door landing with a Play action', () => {
    useGameStore.setState({ status: 'idle', view: null, lobby: null });
    render(<App />);

    expect(screen.getByTestId('connection-screen')).toBeDefined();
    expect(screen.getByTestId('connect-button').textContent).toBe('Play');
  });

  it('walks idle → connecting → open → lobby → first GameView → table', () => {
    const { factory, sockets } = recordingFactory();
    render(<App />);

    // idle: the connection screen is up.
    expect(screen.getByTestId('connection-screen')).toBeDefined();

    // connecting: still the connection screen, now with a Cancel affordance.
    connectWith(factory);
    expect(screen.getByTestId('connection-status').textContent).toContain('Opening a connection');
    expect(screen.getByRole('button', { name: 'Cancel' })).toBeDefined();

    // open, no lobby frame yet: the lobby's waiting fallback (never a dead screen).
    act(() => sockets[0].emitOpen());
    expect(screen.getByTestId('lobby-waiting')).toBeDefined();
    expect(screen.getByTestId('lobby-disconnect-button')).toBeDefined();
    expect(screen.queryByTestId('connection-screen')).toBeNull();

    // first LobbyView: the interactive lobby replaces the fallback.
    act(() => sockets[0].emitMessage(LOBBY_ROOMLESS_JSON));
    expect(screen.getByTestId('lobby-screen')).toBeDefined();
    expect(screen.queryByTestId('lobby-waiting')).toBeNull();

    // first GameView (game constructed): the 2.5D match table replaces the lobby.
    act(() => sockets[0].emitMessage(SAMPLE_GAME_VIEW_JSON));
    expect(screen.getByTestId('live-match-table')).toBeDefined();
    expect(screen.getByTestId('live-2-5d-plane')).toBeDefined();
    expect(screen.queryByTestId('lobby-screen')).toBeNull();
  });

  it('the lobby waiting fallback can disconnect back to the connection screen', () => {
    const { factory, sockets } = recordingFactory();
    render(<App />);

    connectWith(factory);
    act(() => sockets[0].emitOpen());
    expect(screen.getByTestId('lobby-waiting')).toBeDefined();

    fireEvent.click(screen.getByTestId('lobby-disconnect-button'));
    expect(screen.getByTestId('connection-screen')).toBeDefined();
  });

  it('disconnecting from an in-room lobby returns to the connection screen', () => {
    const { factory, sockets } = recordingFactory();
    render(<App />);

    connectWith(factory);
    act(() => sockets[0].emitOpen());
    act(() => sockets[0].emitMessage(LOBBY_ROOMLESS_JSON));
    expect(screen.getByTestId('lobby-screen')).toBeDefined();

    fireEvent.click(screen.getByTestId('lobby-disconnect-button'));
    expect(screen.getByTestId('connection-screen')).toBeDefined();
    expect(screen.queryByTestId('lobby-screen')).toBeNull();
  });

  it('leaves a finished game and lands back in the lobby (issue #452)', () => {
    vi.useFakeTimers();
    try {
      const { factory, sockets } = recordingFactory();
      render(<App />);

      connectWith(factory);
      act(() => sockets[0].emitOpen());
      act(() => sockets[0].emitMessage(LOBBY_ROOMLESS_JSON));
      act(() => sockets[0].emitMessage(SAMPLE_GAME_VIEW_JSON));
      // The game ends (this frame is equally the one a reconnect into a finished
      // game replays, and the one a concede produces — there is no special case).
      act(() => sockets[0].emitMessage(GAME_OVER_LOSS_JSON));
      expect(screen.getByTestId('game-over-overlay')).toBeDefined();

      // The verdict's exit routes back out; nothing pins the app to the dead
      // screen. The scene first recedes into the lobby — the §8 "Return to
      // lobby" moment (issue #509) — and the store transition runs when it
      // lands, so the exit is one bounded ≤ 400 ms hand-off, not a hang.
      fireEvent.click(screen.getByTestId('game-over-leave'));
      act(() => {
        vi.advanceTimersByTime(400);
      });
      expect(screen.queryByTestId('game-over-overlay')).toBeNull();
      expect(screen.queryByTestId('live-match-table')).toBeNull();
      act(() => sockets[1].emitOpen());
      act(() => sockets[1].emitMessage(LOBBY_ROOMLESS_JSON));
      expect(screen.getByTestId('lobby-screen')).toBeDefined();
    } finally {
      vi.useRealTimers();
    }
  });

  it('shows a retry after the connection closes (error surfaces as a close)', () => {
    const { factory, sockets } = recordingFactory();
    render(<App />);

    connectWith(factory);
    // A failed/dropped connection surfaces as a close; the store has no 'error'.
    act(() => sockets[0].drop());

    expect(screen.getByRole('alert').textContent).toContain('Connection closed');
    expect(screen.getByTestId('connect-button').textContent).toBe('Retry');
  });
});
