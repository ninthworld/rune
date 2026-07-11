import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { createGameStore, selectPendingPrompt, type SocketFactory } from './store';
import { normalizeGameView } from './wire';
import { SAMPLE_GAME_VIEW, SAMPLE_GAME_VIEW_JSON } from './game-view.fixture';

/**
 * A structural stand-in for the browser `WebSocket`, driven manually so tests
 * control open/message/close timing without real I/O.
 */
class FakeSocket {
  onopen: ((event: unknown) => void) | null = null;
  onmessage: ((event: { data: unknown }) => void) | null = null;
  onclose: ((event: unknown) => void) | null = null;
  onerror: ((event: unknown) => void) | null = null;
  readonly sent: string[] = [];
  closed = false;

  send(data: string): void {
    this.sent.push(data);
  }

  close(): void {
    this.closed = true;
    this.onclose?.({});
  }

  // Server-side conveniences for tests.
  emitOpen(): void {
    this.onopen?.({});
  }
  emitMessage(data: string): void {
    this.onmessage?.({ data });
  }
  drop(): void {
    // A network drop: onclose fires without an intentional close() call.
    this.onclose?.({});
  }
}

/** A socket factory that records every socket it hands out. */
function recordingFactory(): { factory: SocketFactory; sockets: FakeSocket[] } {
  const sockets: FakeSocket[] = [];
  const factory: SocketFactory = () => {
    const s = new FakeSocket();
    sockets.push(s);
    return s as unknown as WebSocket;
  };
  return { factory, sockets };
}

describe('game store', () => {
  it('reconstructs complete state from a single GameView (no prior messages)', () => {
    const store = createGameStore();
    // Feed exactly one frame into a pristine store.
    store.getState().ingest(SAMPLE_GAME_VIEW_JSON);

    const { view } = store.getState();
    expect(view).toEqual(SAMPLE_GAME_VIEW);

    // The pending prompt is fully derivable from that one view — nothing else.
    const prompt = selectPendingPrompt(view);
    expect(prompt).not.toBeNull();
    expect(prompt?.actions).toHaveLength(2);
    expect(prompt?.globalActions.map((a) => a.id)).toEqual(['a1']);
    expect(prompt?.subjectActions.map((a) => a.id)).toEqual(['a2']);
    expect(prompt?.deadline).toBe(12.5);
  });

  it('replaces the prior view wholesale — no diff/merge', () => {
    const store = createGameStore();
    store.getState().ingest(SAMPLE_GAME_VIEW_JSON);
    expect(store.getState().view?.battlefield).toHaveLength(1);

    // A later, sparser frame must fully supersede the earlier one.
    store.getState().ingest('{"phase":"end"}');
    const { view } = store.getState();
    expect(view).toEqual(normalizeGameView({ phase: 'end' }));
    expect(view?.battlefield).toEqual([]);
    expect(view?.my_hand).toEqual([]);
    expect(selectPendingPrompt(view)).toBeNull();
  });

  it('sends a ChooseAction echoing the chosen id', () => {
    const store = createGameStore();
    const { factory, sockets } = recordingFactory();
    store.getState().connect('ws://test', { createSocket: factory, autoReconnect: false });
    sockets[0].emitOpen();
    sockets[0].emitMessage(SAMPLE_GAME_VIEW_JSON);

    store.getState().choose('a2');
    expect(sockets[0].sent).toEqual([JSON.stringify({ type: 'choose_action', action_id: 'a2' })]);
  });

  it('tracks connection status through the lifecycle', () => {
    const store = createGameStore();
    const { factory, sockets } = recordingFactory();
    expect(store.getState().status).toBe('idle');

    store.getState().connect('ws://test', { createSocket: factory, autoReconnect: false });
    expect(store.getState().status).toBe('connecting');

    sockets[0].emitOpen();
    expect(store.getState().status).toBe('open');

    store.getState().disconnect();
    expect(store.getState().status).toBe('closed');
  });

  describe('reconnect', () => {
    beforeEach(() => vi.useFakeTimers());
    afterEach(() => vi.useRealTimers());

    it('relies on the server re-sending full state; the fresh view replaces', () => {
      const store = createGameStore();
      const { factory, sockets } = recordingFactory();
      store.getState().connect('ws://test', { createSocket: factory, reconnectDelayMs: 10 });

      sockets[0].emitOpen();
      sockets[0].emitMessage(SAMPLE_GAME_VIEW_JSON);
      expect(store.getState().view).toEqual(SAMPLE_GAME_VIEW);

      // Connection drops unexpectedly.
      sockets[0].drop();
      expect(store.getState().status).toBe('closed');

      // Auto-reconnect fires and a brand-new socket is opened.
      vi.advanceTimersByTime(10);
      expect(sockets).toHaveLength(2);

      // The server resends full state; it replaces the prior view outright.
      const resent = normalizeGameView({ phase: 'end', mana_pool: ['{U}'] });
      sockets[1].emitOpen();
      sockets[1].emitMessage(JSON.stringify(resent));
      expect(store.getState().view).toEqual(resent);
    });

    it('does not reconnect after an intentional disconnect', () => {
      const store = createGameStore();
      const { factory, sockets } = recordingFactory();
      store.getState().connect('ws://test', { createSocket: factory, reconnectDelayMs: 10 });
      sockets[0].emitOpen();

      store.getState().disconnect();
      vi.advanceTimersByTime(1000);
      expect(sockets).toHaveLength(1);
    });
  });

  it('never writes game state to localStorage', () => {
    const setItem = vi.fn();
    vi.stubGlobal('localStorage', {
      setItem,
      getItem: vi.fn(),
      removeItem: vi.fn(),
      clear: vi.fn(),
    });

    const store = createGameStore();
    const { factory, sockets } = recordingFactory();
    store.getState().connect('ws://test', { createSocket: factory, autoReconnect: false });
    sockets[0].emitOpen();
    sockets[0].emitMessage(SAMPLE_GAME_VIEW_JSON);
    store.getState().choose('a2');

    expect(setItem).not.toHaveBeenCalled();
    vi.unstubAllGlobals();
  });
});
