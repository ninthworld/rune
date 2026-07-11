/**
 * The RUNE web client's networking store (Zustand).
 *
 * This is the single place client networking state lives. It owns the WebSocket
 * connection, holds the **latest** {@link GameView} as the sole source of UI
 * truth, and sends `ChooseAction` when the player picks one of the server-issued
 * `valid_actions`. Per the hard rules (AGENTS.md):
 *
 * - Zero game logic: the store never computes legality, cost, or effect. It
 *   echoes back the `id` of an action the server already offered.
 * - The whole UI must be reconstructable from a single `GameView` — so a new
 *   view **replaces** the prior one wholesale (no diff/merge). Reconnect relies
 *   on the server re-sending full state; there is nothing to reconcile.
 * - No game state in `localStorage`; the server is the source of truth.
 *
 * The "pending prompt" is not stored separately — it is a pure derivation of the
 * latest `GameView` (see {@link selectPendingPrompt}), which keeps the GameView
 * the one and only load-bearing piece of state.
 */
import { create, type StateCreator } from 'zustand';
import { createStore } from 'zustand/vanilla';
import { chooseAction, type GameView, type ValidAction } from './protocol';
import { parseGameView } from './wire';

/** Connection lifecycle for status display only (never load-bearing game state). */
export type ConnectionStatus = 'idle' | 'connecting' | 'open' | 'closed';

/** Constructs the transport socket. Injectable so the store is testable. */
export type SocketFactory = (url: string) => WebSocket;

/** Options for {@link GameStore.connect}. */
export interface ConnectOptions {
  /** Socket constructor; defaults to the browser `WebSocket`. */
  createSocket?: SocketFactory;
  /** Reconnect after an unexpected close. Defaults to `true`. */
  autoReconnect?: boolean;
  /** Fixed delay before a reconnect attempt, in ms. Defaults to `1000`. */
  reconnectDelayMs?: number;
}

/** The networking store's shape. */
export interface GameStore {
  /** The latest personalized view, or `null` before the first message. */
  view: GameView | null;
  /** Current connection lifecycle state. */
  status: ConnectionStatus;
  /** Open (or replace) the connection to `url`. */
  connect: (url: string, options?: ConnectOptions) => void;
  /** Close the connection intentionally; suppresses auto-reconnect. */
  disconnect: () => void;
  /** Send a `ChooseAction` for one of the currently issued `valid_actions`. */
  choose: (actionId: string) => void;
  /**
   * Ingest one raw server frame, replacing the stored view. This is the single
   * entry point for server→client state and the seam tests use to feed a lone
   * `GameView` (the reconstruct-from-one-GameView invariant).
   */
  ingest: (raw: string) => void;
}

const defaultSocketFactory: SocketFactory = (url) => new WebSocket(url);

const initializer: StateCreator<GameStore> = (set, get) => {
  // Transport handles live in this per-store closure, never in the state tree —
  // they are not part of the reconstructable UI.
  let socket: WebSocket | null = null;
  let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  let intentionalClose = false;
  let lastUrl: string | null = null;
  let lastOptions: ConnectOptions = {};

  const clearReconnect = (): void => {
    if (reconnectTimer !== null) {
      clearTimeout(reconnectTimer);
      reconnectTimer = null;
    }
  };

  const open = (url: string, options: ConnectOptions): void => {
    lastUrl = url;
    lastOptions = options;
    intentionalClose = false;
    set({ status: 'connecting' });

    const factory = options.createSocket ?? defaultSocketFactory;
    const s = factory(url);
    socket = s;

    s.onopen = (): void => {
      if (socket === s) set({ status: 'open' });
    };
    s.onmessage = (event: MessageEvent): void => {
      // Only text frames carry the protocol; ignore binary.
      if (typeof event.data === 'string') get().ingest(event.data);
    };
    s.onclose = (): void => {
      if (socket !== s) return; // a superseded socket closing; ignore.
      socket = null;
      set({ status: 'closed' });
      const autoReconnect = options.autoReconnect ?? true;
      if (!intentionalClose && autoReconnect && lastUrl !== null) {
        clearReconnect();
        const delay = options.reconnectDelayMs ?? 1000;
        reconnectTimer = setTimeout(() => {
          reconnectTimer = null;
          open(lastUrl!, lastOptions);
        }, delay);
      }
    };
    s.onerror = (): void => {
      // Errors surface as a subsequent close; nothing load-bearing to record.
    };
  };

  return {
    view: null,
    status: 'idle',

    connect(url, options = {}): void {
      clearReconnect();
      if (socket) {
        intentionalClose = true;
        socket.close();
        socket = null;
      }
      open(url, options);
    },

    disconnect(): void {
      intentionalClose = true;
      clearReconnect();
      if (socket) {
        socket.close();
        socket = null;
      }
      set({ status: 'closed' });
    },

    choose(actionId): void {
      // Echo the chosen action id; the server validates it against what it
      // issued. No legality is computed here (hard rule).
      if (!socket) return;
      socket.send(JSON.stringify(chooseAction(actionId)));
    },

    ingest(raw): void {
      // Full replace: the fresh GameView is the complete UI state. No merge with
      // any prior view — that is what makes reconnect/resync trivially correct.
      set({ view: parseGameView(raw) });
    },
  };
};

/**
 * A pending decision derived purely from the latest {@link GameView}. This is
 * presentation grouping (global vs entity-subject actions, per ADR 0004), not
 * game logic — the actions themselves are exactly what the server issued.
 */
export interface PendingPrompt {
  /** Every action the server currently offers, in issued order. */
  actions: ValidAction[];
  /** Subject-less actions (pass, end turn) — the action bar. */
  globalActions: ValidAction[];
  /** Actions bound to one or more entities — rendered on those entities. */
  subjectActions: ValidAction[];
  /** Seconds remaining for the decision, if a clock is running. */
  deadline?: number;
}

/**
 * Derive the pending prompt from a view. Returns `null` when there is nothing
 * for the receiving player to decide (no issued actions).
 */
export function selectPendingPrompt(view: GameView | null): PendingPrompt | null {
  if (!view || view.valid_actions.length === 0) return null;
  const globalActions = view.valid_actions.filter((a) => !a.subject || a.subject.length === 0);
  const subjectActions = view.valid_actions.filter((a) => a.subject && a.subject.length > 0);
  return {
    actions: view.valid_actions,
    globalActions,
    subjectActions,
    deadline: view.action_deadline,
  };
}

/** Create an isolated store instance (used by tests and non-React consumers). */
export function createGameStore() {
  return createStore<GameStore>()(initializer);
}

/** The app-wide store hook. React components subscribe via `useGameStore(...)`. */
export const useGameStore = create<GameStore>()(initializer);
