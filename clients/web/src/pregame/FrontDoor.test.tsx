/**
 * The front door's reconnect copy (issue #506; `front-door-and-lobby.md` §5.1,
 * §8 criterion 14) — P11's fix.
 *
 * A returning player used to see the same generic "Opening a connection to …"
 * as a cold start. The socket lifecycle is unchanged; only the words are, and
 * only when `restoreSession()` actually had a stored session for this address.
 *
 * The rest of the front door's behavior is covered by the shipped
 * `ConnectionScreen.test.tsx`, which passes unmigrated through the restyle.
 */
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { act, cleanup, render, screen } from '@testing-library/react';
import { ConnectionScreen } from '../ConnectionScreen';
import { useGameStore, type SocketFactory } from '../store';

const TOKEN_KEY = 'rune.session.token';
const URL_KEY = 'rune.session.url';

/** A socket that never opens, so the screen stays in `connecting`. */
class PendingSocket {
  onopen: ((event: unknown) => void) | null = null;
  onmessage: ((event: { data: unknown }) => void) | null = null;
  onclose: ((event: unknown) => void) | null = null;
  onerror: ((event: unknown) => void) | null = null;
  send(): void {}
  close(): void {
    this.onclose?.({});
  }
}

const factory: SocketFactory = () => new PendingSocket() as unknown as WebSocket;

beforeEach(() => sessionStorage.clear());

afterEach(() => {
  cleanup();
  act(() => useGameStore.getState().disconnect());
  useGameStore.setState({ status: 'idle', view: null, lobby: null, reclaimingSession: false });
  sessionStorage.clear();
});

describe('Front door — criterion 14: reclaiming vs connecting', () => {
  it('reads “Reclaiming your seat” when restoreSession had a stored session', () => {
    sessionStorage.setItem(TOKEN_KEY, 's:held');
    sessionStorage.setItem(URL_KEY, 'ws://seat:9000');

    act(() => {
      useGameStore.getState().restoreSession({ createSocket: factory, autoReconnect: false });
    });
    render(<ConnectionScreen />);

    expect(screen.getByTestId('connection-status').textContent).toContain('Reclaiming your seat');
    // The pill keeps the connecting treatment; only the word changes.
    expect(screen.getByTestId('connection-screen').textContent).toContain('Reclaiming');
    // Still never a dead screen.
    expect(screen.getByRole('button', { name: 'Cancel' })).toBeDefined();
  });

  it('reads “Opening a connection” for an ordinary, user-driven connect', () => {
    act(() => {
      useGameStore.getState().connect('ws://fresh:9000', {
        createSocket: factory,
        autoReconnect: false,
      });
    });
    render(<ConnectionScreen />);

    expect(screen.getByTestId('connection-status').textContent).toContain('Opening a connection');
    expect(screen.getByTestId('connection-status').textContent).not.toContain('Reclaiming');
  });

  it('drops the reclaim state as soon as the socket answers', () => {
    sessionStorage.setItem(TOKEN_KEY, 's:held');
    sessionStorage.setItem(URL_KEY, 'ws://seat:9000');
    act(() => {
      useGameStore.getState().restoreSession({ createSocket: factory, autoReconnect: false });
    });
    expect(useGameStore.getState().reclaimingSession).toBe(true);

    // Where the reclaimed session lands is entirely the server's answer; the
    // client's reclaim copy is over the moment the transport resolves.
    act(() => useGameStore.getState().disconnect());
    expect(useGameStore.getState().reclaimingSession).toBe(false);
  });
});

describe('Front door — one gold, and settings preserved (criteria 10, 18)', () => {
  it('spends exactly one gold on Play and keeps the Display settings button', () => {
    useGameStore.setState({ status: 'idle', view: null, lobby: null });
    const { container } = render(<ConnectionScreen />);

    const gold = container.querySelectorAll('[data-gold="true"]');
    expect(gold).toHaveLength(1);
    expect((gold[0] as HTMLElement).dataset.testid ?? gold[0]!.getAttribute('data-testid')).toBe(
      'connect-button',
    );
    expect(screen.getByTestId('front-door-settings')).toBeDefined();
  });
});
