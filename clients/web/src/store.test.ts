import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { createGameStore, selectPendingPrompt, type SocketFactory } from './store';
import { normalizeGameView } from './wire';
import {
  createRoomCommand,
  joinRoomCommand,
  leaveCommand,
  readyCommand,
  submitDeckCommand,
} from './protocol';
import { SAMPLE_GAME_VIEW, SAMPLE_GAME_VIEW_JSON } from './game-view.fixture';
import {
  LOBBY_ROOMLESS_JSON,
  LOBBY_ROOM_DECKED_JSON,
  LOBBY_ROOM_READY_JSON,
  LOBBY_ROOM_UNDECKED_JSON,
} from './lobby-view.fixture';

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

  it('sends a ChooseAction echoing the chosen id (plain action, no token/targets)', () => {
    const store = createGameStore();
    const { factory, sockets } = recordingFactory();
    store.getState().connect('ws://test', { createSocket: factory, autoReconnect: false });
    sockets[0].emitOpen();
    sockets[0].emitMessage(SAMPLE_GAME_VIEW_JSON);

    store.getState().choose({ id: 'a2', type: 'activate_ability', label: 'Tap for mana' });
    // The socket also carried the lobby `Hello` on open; the ChooseAction is the
    // frame under test here.
    expect(sockets[0].sent).toContain(JSON.stringify({ type: 'choose_action', action_id: 'a2' }));
  });

  it('answers a targeted action atomically: echoes the token and per-slot targets', () => {
    const store = createGameStore();
    const { factory, sockets } = recordingFactory();
    store.getState().connect('ws://test', { createSocket: factory, autoReconnect: false });
    sockets[0].emitOpen();
    sockets[0].emitMessage(SAMPLE_GAME_VIEW_JSON);

    // The action carries the server's content-binding token; the assembled answer
    // is submitted in a single message (never a multi-step handshake).
    store.getState().choose(
      {
        id: 'a3',
        type: 'cast_spell',
        label: 'Cast Lightning Bolt',
        subject: ['c3'],
        token: 'h:9f2c',
        requirements: [{ slot: 't0', prompt: 'target creature', candidates: ['perm_xyz'] }],
      },
      [{ slot: 't0', chosen: ['perm_xyz'] }],
    );
    expect(sockets[0].sent).toContain(
      JSON.stringify({
        type: 'choose_action',
        action_id: 'a3',
        token: 'h:9f2c',
        targets: [{ slot: 't0', chosen: ['perm_xyz'] }],
      }),
    );
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

  describe('lobby (issue #114)', () => {
    /** Connect + open a store with a fake socket, returning the socket. */
    function open(): { store: ReturnType<typeof createGameStore>; socket: FakeSocket } {
      const store = createGameStore();
      const { factory, sockets } = recordingFactory();
      store.getState().connect('ws://test', { createSocket: factory, autoReconnect: false });
      sockets[0].emitOpen();
      return { store, socket: sockets[0] };
    }

    it('greets the server with a Hello on open (no token on first contact)', () => {
      const { socket } = open();
      expect(socket.sent).toEqual([JSON.stringify({ type: 'hello' })]);
    });

    it('routes a LobbyView frame to `lobby` (not `view`)', () => {
      const { store, socket } = open();
      socket.emitMessage(LOBBY_ROOMLESS_JSON);

      expect(store.getState().view).toBeNull();
      expect(store.getState().lobby).toEqual({
        session: 's:ab12',
        you: 'p1',
        directory: [],
        valid_commands: ['create_room', 'join_room'],
      });
    });

    it('transitions to the game (sets `view`) on the first GameView', () => {
      const { store, socket } = open();
      socket.emitMessage(LOBBY_ROOMLESS_JSON);
      expect(store.getState().lobby).not.toBeNull();

      // The game is constructed; the server now speaks GameViews.
      socket.emitMessage(SAMPLE_GAME_VIEW_JSON);
      expect(store.getState().view).toEqual(SAMPLE_GAME_VIEW);
    });

    it('sends lobby commands verbatim', () => {
      const { store, socket } = open();
      store.getState().sendLobby(createRoomCommand({ seats: 4, game_setup: 'ffa-4' }));
      store.getState().sendLobby(joinRoomCommand('r:7f3'));
      store.getState().sendLobby(submitDeckCommand(['thornback_boar', 'thornback_boar', 'forest']));
      store.getState().sendLobby(readyCommand(true));

      expect(socket.sent).toContain(
        JSON.stringify({ type: 'create_room', config: { seats: 4, game_setup: 'ffa-4' } }),
      );
      expect(socket.sent).toContain(JSON.stringify({ type: 'join_room', room_id: 'r:7f3' }));
      expect(socket.sent).toContain(
        JSON.stringify({
          type: 'submit_deck',
          cards: ['thornback_boar', 'thornback_boar', 'forest'],
        }),
      );
      expect(socket.sent).toContain(JSON.stringify({ type: 'ready', ready: true }));
    });

    it('clears any error when a command takes effect (create → in a room)', () => {
      const { store, socket } = open();
      socket.emitMessage(LOBBY_ROOMLESS_JSON);
      store.getState().sendLobby(createRoomCommand({ seats: 2, game_setup: '1v1' }));
      // The server confirms with a room-bearing LobbyView.
      socket.emitMessage(LOBBY_ROOM_UNDECKED_JSON);

      expect(store.getState().lobbyError).toBeNull();
      expect(store.getState().lobby?.room?.room_id).toBe('r:7f3');
    });

    it('surfaces a non-fatal, retryable error when a join is rejected', () => {
      const { store, socket } = open();
      socket.emitMessage(LOBBY_ROOMLESS_JSON);
      store.getState().sendLobby(joinRoomCommand('r:nope'));
      // Rejected: the server re-sends the current (still room-less) LobbyView.
      socket.emitMessage(LOBBY_ROOMLESS_JSON);

      expect(store.getState().lobbyError).toContain('Could not join');
      // The lobby is still interactive (create/join still offered) — retry-able.
      expect(store.getState().lobby?.valid_commands).toContain('join_room');
    });

    it('flags a rejected deck, then clears it on a successful resubmit', () => {
      const { store, socket } = open();
      socket.emitMessage(LOBBY_ROOM_UNDECKED_JSON);

      // Submit a deck; the server rejects it (re-sends the undecked view).
      store.getState().sendLobby(submitDeckCommand(['forest']));
      socket.emitMessage(LOBBY_ROOM_UNDECKED_JSON);
      expect(store.getState().lobbyError).toContain('deck was rejected');

      // Resubmit; this time the seat comes back decked and the error clears.
      store.getState().sendLobby(submitDeckCommand(['thornback_boar', 'forest']));
      socket.emitMessage(LOBBY_ROOM_DECKED_JSON);
      expect(store.getState().lobbyError).toBeNull();
      expect(store.getState().lobby?.room?.seats[0].decked).toBe(true);
    });

    it('reconciles ready → the seat reads ready with no error', () => {
      const { store, socket } = open();
      socket.emitMessage(LOBBY_ROOM_DECKED_JSON);
      store.getState().sendLobby(readyCommand(true));
      socket.emitMessage(LOBBY_ROOM_READY_JSON);

      expect(store.getState().lobbyError).toBeNull();
      expect(store.getState().lobby?.room?.seats[0].ready).toBe(true);
    });

    it('drops lobby state on disconnect (returns to an interactive screen)', () => {
      const { store, socket } = open();
      socket.emitMessage(LOBBY_ROOM_UNDECKED_JSON);
      expect(store.getState().lobby).not.toBeNull();

      store.getState().disconnect();
      expect(store.getState().lobby).toBeNull();
      expect(store.getState().lobbyError).toBeNull();
      expect(store.getState().status).toBe('closed');
    });

    it('echoes the last session token on a reconnecting Hello', () => {
      vi.useFakeTimers();
      const store = createGameStore();
      const { factory, sockets } = recordingFactory();
      store.getState().connect('ws://test', { createSocket: factory, reconnectDelayMs: 10 });
      sockets[0].emitOpen();
      // The first LobbyView issues the session token.
      sockets[0].emitMessage(LOBBY_ROOMLESS_JSON);

      // The socket drops; auto-reconnect opens a fresh socket that re-greets.
      sockets[0].drop();
      vi.advanceTimersByTime(10);
      expect(sockets).toHaveLength(2);
      sockets[1].emitOpen();

      expect(sockets[1].sent).toEqual([JSON.stringify({ type: 'hello', token: 's:ab12' })]);
      vi.useRealTimers();
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
    store.getState().choose({ id: 'a2', type: 'activate_ability', label: 'Tap for mana' });

    expect(setItem).not.toHaveBeenCalled();
    vi.unstubAllGlobals();
  });

  describe('session persistence across reload (issue #254)', () => {
    const TOKEN_KEY = 'rune.session.token';
    const URL_KEY = 'rune.session.url';
    beforeEach(() => sessionStorage.clear());
    afterEach(() => sessionStorage.clear());

    /** Connect a fresh store and receive the first LobbyView (which issues the token). */
    function connectAndSeat(url = 'ws://seat'): {
      store: ReturnType<typeof createGameStore>;
      sockets: FakeSocket[];
    } {
      const store = createGameStore();
      const { factory, sockets } = recordingFactory();
      store.getState().connect(url, { createSocket: factory, autoReconnect: false });
      sockets[0].emitOpen();
      sockets[0].emitMessage(LOBBY_ROOMLESS_JSON); // issues session s:ab12
      return { store, sockets };
    }

    it('persists the session token and server URL after the first LobbyView', () => {
      connectAndSeat('ws://seat');
      expect(sessionStorage.getItem(TOKEN_KEY)).toBe('s:ab12');
      expect(sessionStorage.getItem(URL_KEY)).toBe('ws://seat');
    });

    it('reclaims the seat from a fresh store instance (a hard reload)', () => {
      // First "page load": connect and receive the token, persisting it per-tab.
      connectAndSeat('ws://seat');

      // The reload: a brand-new store with no in-memory token restores from
      // sessionStorage and re-greets the server echoing the token to reclaim its seat.
      const reloaded = createGameStore();
      const { factory, sockets } = recordingFactory();
      const attempted = reloaded.getState().restoreSession({ createSocket: factory });

      expect(attempted).toBe(true);
      sockets[0].emitOpen();
      expect(sockets[0].sent).toEqual([JSON.stringify({ type: 'hello', token: 's:ab12' })]);
    });

    it('restoreSession is a no-op when nothing is persisted', () => {
      const store = createGameStore();
      const { factory, sockets } = recordingFactory();
      expect(store.getState().restoreSession({ createSocket: factory })).toBe(false);
      expect(sockets).toHaveLength(0);
    });

    it('a fresh connect takes a new seat rather than reclaiming (two tabs, two seats)', () => {
      // A token is persisted this tab...
      sessionStorage.setItem(TOKEN_KEY, 's:ab12');
      sessionStorage.setItem(URL_KEY, 'ws://seat');
      // ...but connect() greets WITHOUT a token, so a manually-opened second tab gets
      // its own seat. Only restoreSession reclaims — this is why two tabs stay distinct.
      const store = createGameStore();
      const { factory, sockets } = recordingFactory();
      store.getState().connect('ws://seat', { createSocket: factory, autoReconnect: false });
      sockets[0].emitOpen();
      expect(sockets[0].sent).toEqual([JSON.stringify({ type: 'hello' })]);
    });

    it('clears the persisted credential on intentional disconnect', () => {
      const { store } = connectAndSeat();
      expect(sessionStorage.getItem(TOKEN_KEY)).toBe('s:ab12');

      store.getState().disconnect();
      expect(sessionStorage.getItem(TOKEN_KEY)).toBeNull();
      expect(sessionStorage.getItem(URL_KEY)).toBeNull();
      // A later reload then has nothing to restore.
      expect(createGameStore().getState().restoreSession()).toBe(false);
    });

    it('clears the persisted credential on leaving a room', () => {
      const { store } = connectAndSeat();
      expect(sessionStorage.getItem(TOKEN_KEY)).toBe('s:ab12');

      store.getState().sendLobby(leaveCommand());
      expect(sessionStorage.getItem(TOKEN_KEY)).toBeNull();
      expect(sessionStorage.getItem(URL_KEY)).toBeNull();
    });

    it('persists only the credential and URL — never game or UI state', () => {
      const { sockets } = connectAndSeat();
      // A full game view arrives; none of it is persisted.
      sockets[0].emitMessage(SAMPLE_GAME_VIEW_JSON);

      expect(sessionStorage.length).toBe(2);
      expect([sessionStorage.key(0), sessionStorage.key(1)].sort()).toEqual(
        [TOKEN_KEY, URL_KEY].sort(),
      );
    });
  });
});
