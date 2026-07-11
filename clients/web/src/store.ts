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
import {
  chooseAction,
  helloCommand,
  type GameView,
  type LobbyCommand,
  type LobbyView,
  type PlayerId,
  type SeatView,
  type TargetChoice,
  type ValidAction,
} from './protocol';
import { parseServerFrame } from './wire';

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

/**
 * The lobby command kind currently awaiting confirmation, tracked so the store
 * can surface a **non-fatal** error when the server rejects it (an invalid lobby
 * command re-sends the current `LobbyView` unchanged — ADR 0012 — so a rejection
 * is inferred from "the expected change did not happen", not an error frame). A
 * `ready`/`unready` distinction is kept because both are the same wire command.
 */
type PendingLobbyKind = 'create_room' | 'join_room' | 'submit_deck' | 'ready' | 'unready' | 'leave';

/** The player's seat in a lobby view, matched by public identity, if any. */
function seatOf(view: LobbyView, you: PlayerId): SeatView | undefined {
  return view.room?.seats.find((seat) => seat.occupied_by === you);
}

/**
 * Whether a fresh {@link LobbyView} reflects the expected effect of a pending
 * command. This is presentation reconciliation (did my last action take?), never
 * game logic — the server remains the sole authority and this only decides
 * whether to show a retry hint.
 */
function lobbyCommandSatisfied(kind: PendingLobbyKind, view: LobbyView): boolean {
  const seat = seatOf(view, view.you);
  switch (kind) {
    case 'create_room':
    case 'join_room':
      return view.room !== undefined;
    case 'leave':
      return view.room === undefined;
    case 'submit_deck':
      return seat?.decked === true;
    case 'ready':
      return seat?.ready === true;
    case 'unready':
      return seat !== undefined && seat.ready !== true;
  }
}

/** The non-fatal, retryable message shown when a pending command was rejected. */
function lobbyErrorMessage(kind: PendingLobbyKind): string {
  switch (kind) {
    case 'create_room':
      return 'Could not create the room. Check the settings and try again.';
    case 'join_room':
      return 'Could not join that room — it may be full or unknown. Check the id and try again.';
    case 'submit_deck':
      return 'That deck was rejected. Pick a deck and submit again.';
    case 'ready':
      return 'Could not ready up. Try again.';
    case 'unready':
      return 'Could not update readiness. Try again.';
    case 'leave':
      return 'Could not leave the room. Try again.';
  }
}

/** Map an outgoing {@link LobbyCommand} to the pending kind we reconcile against. */
function pendingKindOf(command: LobbyCommand): PendingLobbyKind | null {
  switch (command.type) {
    case 'create_room':
      return 'create_room';
    case 'join_room':
      return 'join_room';
    case 'submit_deck':
      return 'submit_deck';
    case 'ready':
      return command.ready ? 'ready' : 'unready';
    case 'leave':
      return 'leave';
    case 'hello':
      // Identity always succeeds; nothing to reconcile.
      return null;
  }
}

/** The networking store's shape. */
export interface GameStore {
  /** The latest personalized view, or `null` before the first message. */
  view: GameView | null;
  /**
   * The latest pre-game {@link LobbyView}, or `null` when not in the lobby phase.
   * The whole pre-game UI is reconstructable from this one value (ADR 0012); it
   * is replaced wholesale on every lobby frame, exactly like {@link view}.
   */
  lobby: LobbyView | null;
  /**
   * A non-fatal, retryable lobby error to surface (e.g. room full/unknown, deck
   * rejected), or `null`. Ephemeral feedback only — never load-bearing: the
   * interactive lobby UI rebuilds from {@link lobby} alone without it.
   */
  lobbyError: string | null;
  /** Current connection lifecycle state. */
  status: ConnectionStatus;
  /** Open (or replace) the connection to `url`. */
  connect: (url: string, options?: ConnectOptions) => void;
  /** Close the connection intentionally; suppresses auto-reconnect. */
  disconnect: () => void;
  /**
   * Send one {@link LobbyCommand} (create/join/submit-deck/ready/leave). The
   * command is recorded so the next `LobbyView` can be reconciled into a
   * non-fatal error if the server rejected it. No legality is computed here — the
   * client only sends commands the server advertised in `valid_commands`.
   */
  sendLobby: (command: LobbyCommand) => void;
  /**
   * Send a `ChooseAction` for one of the currently issued `valid_actions`,
   * answered atomically. The chosen action's content-binding `token` is echoed
   * verbatim, and `targets` (one entry per requirement slot, assembled by the UI
   * from the server's candidates) is submitted in the same message — never a
   * multi-message handshake. No legality is computed here (hard rule).
   */
  choose: (action: ValidAction, targets?: TargetChoice[]) => void;
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
  // The last server-issued session token, echoed on the next `Hello` to reclaim a
  // held-open seat (ADR 0012). Kept in this closure — not in the state tree and
  // never in localStorage — so it is neither load-bearing UI state nor game state.
  let lastSession: string | null = null;
  // The lobby command kind awaiting a server reply, for non-fatal error
  // reconciliation (see PendingLobbyKind). Transient; not part of the state tree.
  let pendingLobby: PendingLobbyKind | null = null;

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
      if (socket !== s) return;
      set({ status: 'open' });
      // Open the lobby handshake: greet the server (echoing a prior session token
      // when reconnecting). The server replies with the first `LobbyView`. This is
      // the pre-game analogue of the connection simply waiting for a `GameView`.
      s.send(JSON.stringify(helloCommand(lastSession ?? undefined)));
    };
    s.onmessage = (event: MessageEvent): void => {
      // Only text frames carry the protocol; ignore binary.
      if (typeof event.data === 'string') get().ingest(event.data);
    };
    s.onclose = (): void => {
      if (socket !== s) return; // a superseded socket closing; ignore.
      socket = null;
      pendingLobby = null;
      // Drop any pre-game lobby state: a closed socket returns to the interactive
      // connection screen (never a dead lobby whose buttons cannot send). In-game
      // `view` is untouched so a reconnecting game still replaces it wholesale.
      set({ status: 'closed', lobby: null, lobbyError: null });
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
    lobby: null,
    lobbyError: null,
    status: 'idle',

    connect(url, options = {}): void {
      clearReconnect();
      if (socket) {
        intentionalClose = true;
        socket.close();
        socket = null;
      }
      // Start the pre-game flow clean: the fresh connection will `Hello` and
      // receive its own first `LobbyView`.
      pendingLobby = null;
      set({ lobby: null, lobbyError: null });
      open(url, options);
    },

    disconnect(): void {
      intentionalClose = true;
      clearReconnect();
      if (socket) {
        socket.close();
        socket = null;
      }
      pendingLobby = null;
      set({ status: 'closed', lobby: null, lobbyError: null });
    },

    sendLobby(command): void {
      if (!socket) return;
      pendingLobby = pendingKindOf(command);
      socket.send(JSON.stringify(command));
    },

    choose(action, targets): void {
      // Echo the chosen action id plus its content-binding token verbatim, and
      // the assembled per-slot targets. The server validates all three against
      // what it issued; no legality is computed here (hard rule).
      if (!socket) return;
      socket.send(JSON.stringify(chooseAction(action.id, action.token, targets)));
    },

    ingest(raw): void {
      // Route the frame: an in-game `GameView` (carries a phase) or a pre-game
      // `LobbyView`. Either way the fresh frame fully replaces prior state — no
      // merge — which is what makes reconnect/resync trivially correct.
      const frame = parseServerFrame(raw);
      if (frame.kind === 'game') {
        // First GameView: the game has been constructed; the app switches to the
        // in-game table (App gates on `view`). No merge with any prior view.
        set({ view: frame.view });
        return;
      }

      const lobby = frame.lobby;
      // Remember the session token to echo on a later reconnecting `Hello`.
      if (lobby.session) lastSession = lobby.session;

      // Reconcile a pending command into a non-fatal error: a rejected command
      // re-sends the current `LobbyView` unchanged (ADR 0012), so if the expected
      // effect is absent we surface a retry hint. Success (or no pending command)
      // clears the hint. This is presentation only — the interactive lobby still
      // rebuilds from `lobby` alone (nothing load-bearing across messages).
      const kind = pendingLobby;
      pendingLobby = null;
      const lobbyError =
        kind === null
          ? get().lobbyError
          : lobbyCommandSatisfied(kind, lobby)
            ? null
            : lobbyErrorMessage(kind);
      set({ lobby, lobbyError });
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
