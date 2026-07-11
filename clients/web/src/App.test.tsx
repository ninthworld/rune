import { afterEach, describe, expect, it } from 'vitest';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { App } from './App';
import { useGameStore, type SocketFactory } from './store';
import { SAMPLE_GAME_VIEW_JSON } from './game-view.fixture';

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

afterEach(() => {
  cleanup();
  act(() => useGameStore.getState().disconnect());
  useGameStore.setState({ status: 'idle', view: null });
});

describe('App connection gating (issue #103)', () => {
  it('cold-starts on the connection screen with a Connect action', () => {
    useGameStore.setState({ status: 'idle', view: null });
    render(<App />);

    expect(screen.getByTestId('connection-screen')).toBeDefined();
    expect(screen.getByTestId('connect-button').textContent).toBe('Connect');
  });

  it('walks idle → connecting → open → first view → table', () => {
    const { factory, sockets } = recordingFactory();
    render(<App />);

    // idle: the connection screen is up.
    expect(screen.getByTestId('connection-screen')).toBeDefined();

    // connecting: still the connection screen, now with a Cancel affordance.
    connectWith(factory);
    expect(screen.getByTestId('connection-status').textContent).toContain('Opening a connection');
    expect(screen.getByRole('button', { name: 'Cancel' })).toBeDefined();

    // open, no view yet: the table's waiting fallback (never a dead screen).
    act(() => sockets[0].emitOpen());
    expect(screen.getByTestId('table-waiting')).toBeDefined();
    expect(screen.getByTestId('disconnect-button')).toBeDefined();
    expect(screen.queryByTestId('connection-screen')).toBeNull();

    // first GameView: the full table replaces the fallback.
    act(() => sockets[0].emitMessage(SAMPLE_GAME_VIEW_JSON));
    expect(screen.getByTestId('action-bar')).toBeDefined();
    expect(screen.queryByTestId('table-waiting')).toBeNull();
  });

  it('the waiting fallback can disconnect back to the connection screen', () => {
    const { factory, sockets } = recordingFactory();
    render(<App />);

    connectWith(factory);
    act(() => sockets[0].emitOpen());
    expect(screen.getByTestId('table-waiting')).toBeDefined();

    fireEvent.click(screen.getByTestId('disconnect-button'));
    expect(screen.getByTestId('connection-screen')).toBeDefined();
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
