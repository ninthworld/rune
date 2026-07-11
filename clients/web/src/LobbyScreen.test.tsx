import { afterEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { LobbyScreen } from './LobbyScreen';
import { useGameStore, type SocketFactory } from './store';
import {
  LOBBY_ROOMLESS_JSON,
  LOBBY_ROOM_ALL_READY_JSON,
  LOBBY_ROOM_DECKED_JSON,
} from './lobby-view.fixture';

/** A manually-driven WebSocket stand-in that records the frames sent to it. */
class FakeSocket {
  onopen: ((event: unknown) => void) | null = null;
  onmessage: ((event: { data: unknown }) => void) | null = null;
  onclose: ((event: unknown) => void) | null = null;
  onerror: ((event: unknown) => void) | null = null;
  readonly sent: string[] = [];

  send(data: string): void {
    this.sent.push(data);
  }
  close(): void {
    this.onclose?.({});
  }
  emitOpen(): void {
    this.onopen?.({});
  }
  emitMessage(data: string): void {
    this.onmessage?.({ data });
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

/**
 * Connect the real store through a fake socket, open it, and push one lobby frame,
 * then render the lobby screen. Returns the socket so a test can assert what the
 * clicks send back (the last frame; the socket also carries the `Hello`).
 */
function mountLobby(frameJson: string): FakeSocket {
  const { factory, sockets } = recordingFactory();
  act(() =>
    useGameStore.getState().connect('ws://test', { createSocket: factory, autoReconnect: false }),
  );
  act(() => sockets[0].emitOpen());
  act(() => sockets[0].emitMessage(frameJson));
  render(<LobbyScreen />);
  return sockets[0];
}

/** The last frame the client sent, parsed. */
function lastSent(socket: FakeSocket): unknown {
  return JSON.parse(socket.sent[socket.sent.length - 1]);
}

afterEach(() => {
  cleanup();
  act(() => useGameStore.getState().disconnect());
  useGameStore.setState({ status: 'idle', view: null, lobby: null, lobbyError: null });
});

describe('LobbyScreen (issue #114)', () => {
  it('shows create + join when room-less; create sends the configured seats/setup', () => {
    const socket = mountLobby(LOBBY_ROOMLESS_JSON);

    expect(screen.getByTestId('create-room')).toBeDefined();
    expect(screen.getByTestId('join-room')).toBeDefined();

    // Pick a seat count, then create.
    fireEvent.change(screen.getByTestId('seat-count-select'), { target: { value: '6' } });
    fireEvent.click(screen.getByTestId('create-room-button'));

    expect(lastSent(socket)).toEqual({
      type: 'create_room',
      config: { seats: 6, game_setup: '1v1' },
    });
  });

  it('validates an empty room id locally (no send) and joins with a real id', () => {
    const socket = mountLobby(LOBBY_ROOMLESS_JSON);
    const sentBefore = socket.sent.length;

    // Empty id: a local, non-fatal validation error and nothing sent.
    fireEvent.click(screen.getByTestId('join-room-button'));
    expect(screen.getByTestId('join-room-error')).toBeDefined();
    expect(socket.sent).toHaveLength(sentBefore);

    // A real id joins.
    fireEvent.change(screen.getByTestId('join-room-input'), { target: { value: '  r:7f3  ' } });
    fireEvent.click(screen.getByTestId('join-room-button'));
    expect(lastSent(socket)).toEqual({ type: 'join_room', room_id: 'r:7f3' });
  });

  it('renders a room: copyable id, seat roster, and only advertised commands', () => {
    mountLobby(LOBBY_ROOM_DECKED_JSON);

    expect(screen.getByTestId('room-id').textContent).toBe('r:7f3');
    // Seat 0 is mine and decked; seat 1 is open.
    expect(screen.getByTestId('seat-0-decked')).toBeDefined();
    expect(screen.queryByTestId('seat-1-decked')).toBeNull();

    // valid_commands = [submit_deck, ready, leave]: ready shown, unready hidden.
    expect(screen.getByTestId('ready-button')).toBeDefined();
    expect(screen.queryByTestId('unready-button')).toBeNull();
    expect(screen.getByTestId('leave-room-button')).toBeDefined();
  });

  it('submits a bundled deck as a non-empty card list', () => {
    const socket = mountLobby(LOBBY_ROOM_DECKED_JSON);
    fireEvent.click(screen.getByTestId('submit-deck-button'));

    const sent = lastSent(socket) as { type: string; cards: string[] };
    expect(sent.type).toBe('submit_deck');
    expect(Array.isArray(sent.cards)).toBe(true);
    expect(sent.cards.length).toBeGreaterThan(0);
  });

  it('readies up', () => {
    const socket = mountLobby(LOBBY_ROOM_DECKED_JSON);
    fireEvent.click(screen.getByTestId('ready-button'));
    expect(lastSent(socket)).toEqual({ type: 'ready', ready: true });
  });

  it('shows per-seat filled/decked/ready for a full room', () => {
    mountLobby(LOBBY_ROOM_ALL_READY_JSON);
    for (const seat of [0, 1]) {
      expect(screen.getByTestId(`seat-${seat}-decked`)).toBeDefined();
      expect(screen.getByTestId(`seat-${seat}-ready`)).toBeDefined();
    }
    // Both ready: only unready/leave remain.
    expect(screen.queryByTestId('ready-button')).toBeNull();
    expect(screen.getByTestId('unready-button')).toBeDefined();
  });

  it('copies the room id to the clipboard', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, 'clipboard', {
      configurable: true,
      value: { writeText },
    });

    mountLobby(LOBBY_ROOM_DECKED_JSON);
    await act(async () => {
      fireEvent.click(screen.getByTestId('copy-room-id-button'));
    });

    expect(writeText).toHaveBeenCalledWith('r:7f3');
    expect(screen.getByTestId('copy-room-id-button').textContent).toBe('Copied');
  });

  it('renders a non-fatal lobby error with the form still interactive', () => {
    mountLobby(LOBBY_ROOMLESS_JSON);
    act(() =>
      useGameStore.setState({
        lobbyError: 'Could not join that room — it may be full or unknown.',
      }),
    );

    expect(screen.getByTestId('lobby-error').textContent).toContain('full or unknown');
    // The join form is still on screen to retry.
    expect(screen.getByTestId('join-room-button')).toBeDefined();
  });
});
