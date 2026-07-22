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
 * - No game state or UI state is persisted; the server is the source of truth.
 *   The one exception is the **session token** (and the server URL to reconnect
 *   to), kept in `sessionStorage` — see {@link persistSession}. That is a reconnect
 *   *credential*, not game or UI state: it is never rendered, nothing is
 *   reconstructed from it, and it does not survive the tab. It exists solely so a
 *   hard page reload can reclaim the held-open seat (ADR 0012, M1 exit criterion),
 *   which is why it does not violate the reconstruct-from-one-`GameView` rule.
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
  requestCatalogCommand,
  setStopsMessage,
  type CatalogView,
  type GameView,
  type LobbyCommand,
  type LobbyView,
  type Phase,
  type PlayerId,
  type SeatView,
  type SpectatorView,
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
type PendingLobbyKind =
  'create_room' | 'join_room' | 'spectate_room' | 'submit_deck' | 'ready' | 'unready' | 'leave';

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
    case 'spectate_room':
      // A successful spectate yields a SpectatorView, not a LobbyView, and clears the
      // pending kind before we get here. So a LobbyView arriving while this is pending
      // means the spectate was rejected (e.g. the room had not started) — unsatisfied.
      return false;
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
    case 'spectate_room':
      return 'Could not spectate that room — it may not have started yet. Try again once it is in progress.';
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
    case 'spectate_room':
      return 'spectate_room';
    case 'submit_deck':
      return 'submit_deck';
    case 'ready':
      return command.ready ? 'ready' : 'unready';
    case 'leave':
      return 'leave';
    case 'add_ai':
    case 'remove_ai':
      // Host-only AI-seat management (issue #415): a rejected command re-sends the view
      // unchanged (the non-fatal pattern), and the host reads the result directly from the
      // roster (the AI seat appears or does not), so there is nothing to reconcile into a
      // retry hint — like `set_name`.
      return null;
    case 'set_name':
      // The requested name is not echoed back for comparison, and a rejected name is
      // simply not stored (the server re-sends the view unchanged, per the non-fatal
      // pattern); there is nothing to reconcile into a retry hint here (issue #294).
      return null;
    case 'request_catalog':
      // The catalog reply is a `CatalogView`, not a `LobbyView`, and never changes lobby
      // state; there is nothing to reconcile into a retry hint (issue #367).
      return null;
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
   * The latest {@link SpectatorView} when this connection is watching as a spectator
   * (ADR 0022, issue #351), or `null` otherwise. Mutually exclusive with {@link view}:
   * a connection is either seated (`view`) or spectating (`spectatorView`). Replaced
   * wholesale on every spectator frame, exactly like {@link view}, so a spectate mode
   * is reconstructable from this one value.
   */
  spectatorView: SpectatorView | null;
  /**
   * The latest pre-game {@link LobbyView}, or `null` when not in the lobby phase.
   * The whole pre-game UI is reconstructable from this one value (ADR 0012); it
   * is replaced wholesale on every lobby frame, exactly like {@link view}.
   */
  lobby: LobbyView | null;
  /**
   * The public card catalog + format deck rules (issue #367), or `null` until it has
   * been requested and received. Static reference data the deck builder (#368) browses,
   * not per-connection lobby state — fetched once with a `request_catalog` command and
   * replaced wholesale on each {@link CatalogView} frame. Kept separate from
   * {@link lobby} because it does not ride the pushed `LobbyView` and does not change
   * with room/seat state.
   */
  catalog: CatalogView | null;
  /**
   * A non-fatal, retryable lobby error to surface (e.g. room full/unknown, deck
   * rejected), or `null`. Ephemeral feedback only — never load-bearing: the
   * interactive lobby UI rebuilds from {@link lobby} alone without it.
   */
  lobbyError: string | null;
  /**
   * A monotonically increasing counter, bumped each time the server pushes a view
   * flagging the receiver's last in-game action as **rejected** (issue #265). It is
   * only a trigger for the transient rejected-action toast — ephemeral and never load
   * bearing: the table rebuilds fully from {@link view} alone, and this is not
   * persisted or reconstructed from anything. Starts at `0`; the toast fires on each
   * increment (a counter, not a boolean, so back-to-back rejections each re-fire it).
   */
  rejectionNonce: number;
  /** Current connection lifecycle state. */
  status: ConnectionStatus;
  /** Open (or replace) the connection to `url`. */
  connect: (url: string, options?: ConnectOptions) => void;
  /**
   * If a session token + URL were persisted this tab (see {@link persistSession}),
   * reconnect to that URL and echo the token on `Hello` to reclaim the held seat —
   * the hard-page-reload path of the M1 exit criterion. Returns `true` when a
   * reconnect was attempted, `false` when there was nothing to restore (so the caller
   * shows the connection screen). A no-op if a socket is already live.
   */
  restoreSession: (options?: ConnectOptions) => boolean;
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
   * Ask the server for the public card catalog + format deck rules (issue #367),
   * the browsable card pool the deck builder (#368) works from. Sends a one-shot
   * `request_catalog`; the reply is a {@link CatalogView} frame that lands in
   * {@link catalog}. It changes no lobby state and needs no reconciliation, so —
   * unlike {@link sendLobby} — it is not recorded as a pending command. A no-op when
   * no socket is open. Idempotent to re-request; callers guard on `catalog === null`
   * to avoid refetching data they already hold.
   */
  requestCatalog: () => void;
  /**
   * Send a `ChooseAction` for one of the currently issued `valid_actions`,
   * answered atomically. The chosen action's content-binding `token` is echoed
   * verbatim, and `targets` (one entry per requirement slot, assembled by the UI
   * from the server's candidates) is submitted in the same message — never a
   * multi-message handshake. No legality is computed here (hard rule).
   */
  choose: (action: ValidAction, targets?: TargetChoice[]) => void;
  /**
   * Set this connection's priority-stop preferences (issue #264): the steps at which
   * the seat wants priority even when idle, so basic auto-pass does not skip it there.
   * Sends a `set_stops` message; the server stores it (surviving reconnect) and
   * reflects it back in `GameView.stops`, which is the sole source of the toggles'
   * rendered state — nothing is stored client-side. No legality is computed here.
   */
  setStops: (stops: Phase[]) => void;
  /**
   * Ingest one raw server frame, replacing the stored view. This is the single
   * entry point for server→client state and the seam tests use to feed a lone
   * `GameView` (the reconstruct-from-one-GameView invariant).
   */
  ingest: (raw: string) => void;
}

const defaultSocketFactory: SocketFactory = (url) => new WebSocket(url);

/** `sessionStorage` key for the reconnect session token (ADR 0012). */
const SESSION_TOKEN_KEY = 'rune.session.token';
/** `sessionStorage` key for the server URL to reconnect the token against. */
const SESSION_URL_KEY = 'rune.session.url';

/**
 * The persisted reconnect credential: a session `token` and the `url` it was issued
 * against, or `null` if none is stored (or storage is unavailable). `sessionStorage`
 * is deliberate — **per tab**: it survives a reload but dies with the tab, so two tabs
 * keep two distinct seats and a closed tab leaves nothing behind. Not `localStorage`
 * (which would share one seat across tabs) and not the state tree (it is a credential,
 * not reconstructable UI). All access is guarded so a storage-less environment (SSR,
 * privacy mode) degrades to "no reconnect" rather than throwing.
 */
function readPersistedSession(): { token: string; url: string } | null {
  try {
    const token = sessionStorage.getItem(SESSION_TOKEN_KEY);
    const url = sessionStorage.getItem(SESSION_URL_KEY);
    if (token && url) return { token, url };
  } catch {
    // storage unavailable — treat as no persisted session.
  }
  return null;
}

/** Persist the reconnect credential (token + the URL it was issued against). */
function persistSession(token: string, url: string): void {
  try {
    sessionStorage.setItem(SESSION_TOKEN_KEY, token);
    sessionStorage.setItem(SESSION_URL_KEY, url);
  } catch {
    // storage unavailable — reconnect-after-reload simply won't be offered.
  }
}

/** Clear the persisted credential so a finished session cannot haunt the next one. */
function clearPersistedSession(): void {
  try {
    sessionStorage.removeItem(SESSION_TOKEN_KEY);
    sessionStorage.removeItem(SESSION_URL_KEY);
  } catch {
    // storage unavailable — nothing to clear.
  }
}

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
    spectatorView: null,
    lobby: null,
    catalog: null,
    lobbyError: null,
    rejectionNonce: 0,
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

    restoreSession(options = {}): boolean {
      // Reload path: nothing to do if we are already connected this tab.
      if (socket !== null) return false;
      const persisted = readPersistedSession();
      if (persisted === null) return false;
      // Prime the token so the reconnecting `Hello` reclaims the held seat, then open
      // the same URL exactly as `connect` would (Hello → first LobbyView/GameView).
      lastSession = persisted.token;
      pendingLobby = null;
      set({ lobby: null, lobbyError: null });
      open(persisted.url, options);
      return true;
    },

    disconnect(): void {
      intentionalClose = true;
      clearReconnect();
      if (socket) {
        socket.close();
        socket = null;
      }
      pendingLobby = null;
      // An intentional disconnect ends the session: drop the persisted credential so a
      // later reload starts fresh at the connection screen (issue #254).
      lastSession = null;
      clearPersistedSession();
      set({ status: 'closed', lobby: null, lobbyError: null });
    },

    sendLobby(command): void {
      if (!socket) return;
      pendingLobby = pendingKindOf(command);
      // Leaving the room gives up the seat, so the reconnect credential is spent:
      // drop it (the live socket keeps working via the in-closure token; only a hard
      // reload is affected, which should then start fresh — issue #254).
      if (pendingLobby === 'leave') {
        lastSession = null;
        clearPersistedSession();
      }
      socket.send(JSON.stringify(command));
    },

    requestCatalog(): void {
      // Fire-and-forget: the catalog is static reference data, not lobby state, so it
      // is neither recorded as a pending command nor reconciled — the reply simply
      // populates `catalog` (see `ingest`). No legality is computed here.
      if (!socket) return;
      socket.send(JSON.stringify(requestCatalogCommand()));
    },

    choose(action, targets): void {
      // Echo the chosen action id plus its content-binding token verbatim, and
      // the assembled per-slot targets. The server validates all three against
      // what it issued; no legality is computed here (hard rule).
      if (!socket) return;
      socket.send(JSON.stringify(chooseAction(action.id, action.token, targets)));
    },

    setStops(stops): void {
      // Send the seat's stop preferences; the server is authoritative and reflects
      // the accepted set back in the next `GameView.stops`. Nothing is stored here —
      // the toggles render from the server's echo, so this survives reconnect.
      if (!socket) return;
      socket.send(JSON.stringify(setStopsMessage(stops)));
    },

    ingest(raw): void {
      // Route the frame: an in-game `GameView` (carries a phase) or a pre-game
      // `LobbyView`. Either way the fresh frame fully replaces prior state — no
      // merge — which is what makes reconnect/resync trivially correct.
      const frame = parseServerFrame(raw);
      if (frame.kind === 'game') {
        // First GameView: the game has been constructed; the app switches to the
        // in-game table (App gates on `view`). No merge with any prior view.
        //
        // A view flagged `action_rejected` (issue #265) means this frame answers a
        // rejected in-game action; bump the ephemeral trigger so the table shows a
        // transient "the game moved on" toast. The flag never survives into stored
        // state — only the counter changes — so the view stays the sole load-bearing
        // truth and a resync (which clears the flag) never re-fires the toast.
        set((state) => ({
          view: frame.view,
          // A seated game frame supersedes any spectator session.
          spectatorView: null,
          rejectionNonce: frame.view.action_rejected
            ? state.rejectionNonce + 1
            : state.rejectionNonce,
        }));
        return;
      }

      if (frame.kind === 'spectator') {
        // A spectator frame (ADR 0022, issue #351): the app switches to the read-only
        // spectate mode (App gates on `spectatorView`). Like a `GameView` it fully
        // replaces prior state — no merge — so a mid-game join or reconnect is trivially
        // correct. A pending `spectate_room` command is satisfied by this frame arriving.
        pendingLobby = null;
        set({ spectatorView: frame.view, view: null, lobby: null, lobbyError: null });
        return;
      }

      if (frame.kind === 'catalog') {
        // A catalog frame (issue #367): static reference data answered to a
        // `request_catalog`. It is not lobby/game state, so it does not touch `view`,
        // `spectatorView`, `lobby`, or the pending-command reconciliation — it only
        // populates `catalog` for the deck builder (#368) to browse.
        set({ catalog: frame.catalog });
        return;
      }

      if (frame.kind === 'lobby_error') {
        // A structured lobby-error frame (issue #395): the server's own human-readable
        // reason a command (e.g. `submit_deck`) was rejected, delivered to this seat
        // only. Surface it verbatim as the non-fatal `lobbyError` and clear the pending
        // command so the unchanged `LobbyView` re-sent alongside it (ADR 0012) does not
        // overwrite this specific reason with a generic inferred hint — the explicit
        // reason wins regardless of frame order. Ephemeral, never load-bearing: the
        // lobby UI still rebuilds from `lobby` alone.
        pendingLobby = null;
        set({ lobbyError: frame.rejection.reason });
        return;
      }

      const lobby = frame.lobby;
      // Remember the session token to echo on a later reconnecting `Hello`, both
      // in-closure (in-page auto-reconnect) and, paired with the server URL, in
      // sessionStorage so a hard page reload can reclaim the same seat (issue #254).
      if (lobby.session) {
        lastSession = lobby.session;
        if (lastUrl !== null) persistSession(lobby.session, lastUrl);
      }

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
