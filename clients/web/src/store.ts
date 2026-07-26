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
import { playerName } from './playerNames';
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
    case 'update_room':
      // Host-only table configuration (issue #546). A rejected update re-sends the view
      // unchanged, and the server explains *why* on the structured `lobby_error` frame
      // (an evicting seat count, an invalid name, a format that refuses the count), so a
      // generic client-side retry hint would only talk over a better message.
      return null;
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

/**
 * The record the lobby's **last-match ribbon** renders after a finished game
 * (`docs/design/front-door-and-lobby.md` §5.5).
 *
 * Presentation-only, explicitly ephemeral state in the exact `lobbyError` idiom:
 * it is written on the same transition that clears {@link GameStore.view}, it is
 * never persisted, and it may never be the only place a piece of information
 * exists. The lobby rebuilds completely from {@link GameStore.lobby} alone with
 * it absent, and no control's availability depends on it — a reload simply loses
 * the ribbon and loses nothing else.
 */
export interface LastMatchSummary {
  /** The outcome family the ribbon tints (the WORD always carries the meaning). */
  outcome: 'victory' | 'defeat' | 'draw';
  /** The opponents' display names, as the finished view named them. */
  opponents: readonly string[];
  /** The finished room's opaque `game_setup` id, for the Play-again pre-fill. */
  gameSetup?: string;
  /** The finished room's seat count, for the Play-again pre-fill. */
  seats?: number;
}

/**
 * Build the ribbon's record from the terminal `GameView` plus the last
 * `LobbyView` still in hand (issue #506; the producer half of
 * `front-door-and-lobby.md` §9 follow-up 2).
 *
 * Pure and total: `null` whenever there is nothing to report — no view, or a
 * view the server has not marked terminal. That covers leaving a *spectated*
 * game (a spectator holds no `view` and played no match) and any future exit
 * from an unfinished one, so the ribbon can never claim a result the server did
 * not decide. **No game logic**: the outcome is a formatting of the server's
 * already-decided `result`, classified exactly as `GameOverOverlay` does.
 *
 * The setup label and seat count come from the room's own `LobbyView`, not the
 * game view — `GameView` deliberately carries no `game_setup` — with the seat
 * count falling back to the view's `seat_order`, which is the same number by
 * construction (a game starts only once every seat is filled).
 */
export function lastMatchOf(
  view: GameView | null,
  lobby: LobbyView | null,
): LastMatchSummary | null {
  const result = view?.result;
  if (view === null || result === undefined) return null;

  const outcome: LastMatchSummary['outcome'] =
    result.winner === undefined ? 'draw' : result.winner === view.you ? 'victory' : 'defeat';

  // Name the opponents the way every other surface does: their chosen display
  // name, else a seat-derived `Player N` from their position in `seat_order` (a
  // real field, never parsed from the opaque id). A player the view names
  // neither way is dropped rather than shown as a raw id — the ribbon would
  // rather say less than print `p2` at someone.
  const opponents = view.opponents
    .map((opponent) => {
      const named = playerName(view, opponent.player_id);
      if (named !== opponent.player_id) return named;
      const seat = view.seat_order.indexOf(opponent.player_id);
      return seat < 0 ? undefined : `Player ${seat + 1}`;
    })
    .filter((name): name is string => name !== undefined);

  const config = lobby?.room?.config;
  return {
    outcome,
    opponents,
    gameSetup: config?.game_setup,
    seats: config?.seats ?? (view.seat_order.length > 0 ? view.seat_order.length : undefined),
  };
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
   * The finished game the lobby's last-match ribbon reports, or `null`. Written
   * on the transition that clears {@link view} (#452/#509 produce it) and by
   * {@link recordLastMatch}; cleared by {@link dismissLastMatch}, by joining or
   * creating any room, and by a reload (it is never persisted). See
   * {@link LastMatchSummary} — ephemeral presentation, never load-bearing.
   */
  lastMatch: LastMatchSummary | null;
  /**
   * Whether the connection currently being opened is a **seat reclaim** — a
   * {@link restoreSession} attempt replaying a stored token — rather than a
   * fresh, user-driven connect. The front door reads it for its connecting copy
   * (`front-door-and-lobby.md` §5.1). Ephemeral connection metadata like
   * {@link status}: nothing is reconstructed from it, and the socket lifecycle
   * is identical either way.
   */
  reclaimingSession: boolean;
  /**
   * A monotonically increasing counter, bumped each time the server pushes a view
   * flagging the receiver's last in-game action as **rejected** (issue #265). It is
   * only a trigger for the transient rejected-action toast — ephemeral and never load
   * bearing: the table rebuilds fully from {@link view} alone, and this is not
   * persisted or reconstructed from anything. Starts at `0`; the toast fires on each
   * increment (a counter, not a boolean, so back-to-back rejections each re-fire it).
   */
  rejectionNonce: number;
  /**
   * The correlation id of the action submission this client is still waiting on
   * (issue #554), or `null` when nothing is in flight. Set when {@link choose} sends a
   * `ChooseAction`, and cleared by the `GameView` whose `action_ack` names *this*
   * submission — never by an unrelated broadcast, which is the whole point: another
   * seat's action pushes a view too, and before the ack existed a pending indicator
   * had no way to tell the two apart.
   *
   * Ephemeral and **never load-bearing**: the table rebuilds fully from {@link view}
   * alone, and it is not persisted. A transport discontinuity — any socket close, and
   * every socket open, the same events {@link GameStore.sessionEpoch} counts — clears
   * it, so a dropped answer can never wedge the UI in "pending". An ack-less view does
   * *not*: an ordinary broadcast carries no ack, and clearing on one would be the
   * pre-#554 heuristic again.
   */
  pendingSubmission: string | null;
  /** Current connection lifecycle state. */
  status: ConnectionStatus;
  /**
   * A monotonically increasing transport generation, bumped once each time a new
   * socket is opened — the first connect, every auto-reconnect after an unexpected
   * close, and a tab-restore {@link GameStore.restoreSession}. It lets a view
   * consumer tell an ordinary in-session update (same epoch as the last view it
   * presented) from a **reconnect/resync discontinuity** (a higher epoch), which is
   * the signal the 2.5D scene uses to choose a full rebuild over an incremental
   * reconcile (issue #493). Ephemeral connection metadata like {@link status}: it is
   * never rendered, nothing is reconstructed from it, and the server's latest
   * `GameView` remains the only durable state — a stale epoch only ever chooses a
   * *harmless* extra rebuild of that same latest view.
   */
  sessionEpoch: number;
  /**
   * The address of the server this connection was opened against, or `null`
   * before the first connect (issue #546). The pregame server plaque names the
   * server a player is on, and a custom address must not be reported as the
   * default one. Ephemeral connection metadata like {@link status}: nothing is
   * reconstructed from it and it is never persisted here — the reclaim path's
   * own copy lives in `sessionStorage` (see {@link restoreSession}).
   */
  serverUrl: string | null;
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
   * Leave the game this connection is in and go back to the lobby (issue #452).
   *
   * This is the counterpart of {@link disconnect} that {@link view} never had: it
   * gives up the seat (closing the socket is what tells the room the seat left) and
   * **clears the terminal view**, so the app's existing gates route on instead of
   * pinning the player to the game-over screen forever. It then reopens the same
   * server so they land back in the lobby; with no server to return to it simply
   * ends at the connection screen. Either way the next screen is interactive.
   *
   * A client-session action like {@link disconnect}, not game state — the terminal
   * `GameView` is still the only thing the game-over screen renders from, and a
   * reconnect that replays it shows exactly the same screen with the same exit.
   *
   * On the way out it also records {@link lastMatch} from the terminal view, which
   * is the lobby's last-match ribbon (issue #506): the reading has to happen here,
   * before the teardown, because the view it comes from is gone afterwards.
   */
  leaveGame: () => void;
  /**
   * Record the finished game the lobby's last-match ribbon reports. Called by
   * {@link leaveGame} on the transition that clears {@link view}, and available
   * to any other producer. Purely ephemeral presentation — see
   * {@link LastMatchSummary}.
   */
  recordLastMatch: (summary: LastMatchSummary) => void;
  /** Drop the last-match ribbon (dismissed, or superseded by a new room). */
  dismissLastMatch: () => void;
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
   * Set this connection's priority-stop preferences (issues #264 and #455): the steps
   * at which the seat wants priority even when idle, so basic auto-pass does not skip
   * it there — `stops` on any turn, `ownTurn` only while the seat is the active
   * player. Sends a `set_stops` message; the server stores it (surviving reconnect)
   * and reflects both halves back in `GameView.stops`/`GameView.own_turn_stops`,
   * which are the sole source of the toggles' rendered state — nothing is stored
   * client-side. No legality is computed here.
   */
  setStops: (stops: Phase[], ownTurn?: Phase[]) => void;
  /**
   * Ingest one raw server frame, replacing the stored view. This is the single
   * entry point for server→client state and the seam tests use to feed a lone
   * `GameView` (the reconstruct-from-one-GameView invariant).
   */
  ingest: (raw: string) => void;
}

/**
 * The still-pending submission id after ingesting `view` (issue #554): `null` once the
 * view acknowledges it, and unchanged when the view answers some *other* submission —
 * which is exactly the case a bare "a view arrived" heuristic got wrong.
 *
 * A view carrying **no ack at all** also leaves it alone. An ordinary broadcast is
 * ack-less — another seat acting pushes a full view to every seat — so clearing on one
 * would reintroduce the very race the correlation exists to remove: this seat's marker
 * would drop the instant an opponent moved, before its own answer arrived.
 *
 * Nothing here can therefore release a marker whose answer is genuinely lost. That is
 * not left to a heuristic: a lost answer means the transport it was owed on is gone, so
 * the marker is cleared on an explicit transport discontinuity (see the socket lifecycle
 * — every open and every close), where the fact is known rather than inferred.
 */
function resolvePending(pending: string | null, view: GameView): string | null {
  if (pending === null) return null;
  const ack = view.action_ack;
  if (ack === undefined) return pending;
  return ack.submission === pending ? null : pending;
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
  // Monotonic source of action-submission correlation ids (issue #554). Opaque to the
  // server, which only echoes them back, so a plain counter is enough; it is scoped to
  // the store instance, not persisted, and never derived from game state.
  let nextSubmission = 0;

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
    // Each opened socket is a new transport generation: bump the epoch so the first
    // view after a (re)connect is recognizable as a discontinuity (issue #493).
    set((state) => ({
      status: 'connecting',
      serverUrl: url,
      sessionEpoch: state.sessionEpoch + 1,
      // A new transport generation cannot answer anything the old one was owed
      // (issue #554). This — not an ack-less broadcast — is the discontinuity that
      // releases the marker, so a dropped answer can never wedge the UI in
      // "pending" while an ordinary broadcast never releases it early.
      pendingSubmission: null,
    }));

    const factory = options.createSocket ?? defaultSocketFactory;
    const s = factory(url);
    socket = s;

    s.onopen = (): void => {
      if (socket !== s) return;
      // The reclaim attempt is over the moment the socket is up: where the
      // session lands is entirely the server's answer in the returned view.
      set({ status: 'open', reclaimingSession: false });
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
      set({
        status: 'closed',
        lobby: null,
        lobbyError: null,
        reclaimingSession: false,
        // The socket a submission was owed its answer on is gone, so nothing can
        // still be in flight over it (issue #554).
        pendingSubmission: null,
      });
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
    lastMatch: null,
    reclaimingSession: false,
    rejectionNonce: 0,
    pendingSubmission: null,
    status: 'idle',
    sessionEpoch: 0,
    serverUrl: null,

    connect(url, options = {}): void {
      clearReconnect();
      if (socket) {
        intentionalClose = true;
        socket.close();
        socket = null;
      }
      // Start the pre-game flow clean: the fresh connection will `Hello` and
      // receive its own first `LobbyView`. A user-driven connect is never a seat
      // reclaim, so the front door shows its ordinary connecting copy.
      pendingLobby = null;
      set({ lobby: null, lobbyError: null, reclaimingSession: false });
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
      // The front door says *Reclaiming your seat* rather than the generic
      // connecting copy while this attempt is in flight (#506, P11). Copy only:
      // the socket lifecycle below is byte-identical to `connect`.
      set({ lobby: null, lobbyError: null, reclaimingSession: true });
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
      // An explicit disconnect ends the session, and the last-match ribbon is
      // session-scoped ephemera: it goes with it (issue #506).
      set({
        status: 'closed',
        lobby: null,
        lobbyError: null,
        reclaimingSession: false,
        lastMatch: null,
      });
    },

    recordLastMatch(summary): void {
      set({ lastMatch: summary });
    },

    dismissLastMatch(): void {
      set({ lastMatch: null });
    },

    leaveGame(): void {
      // Where to come back to, read before the disconnect clears the transport.
      const url = lastUrl;
      const options = lastOptions;
      // Read the ribbon's record NOW, while the terminal view and the room's
      // last `LobbyView` are both still in hand — the teardown below destroys
      // both (issue #506, closing front-door-and-lobby §9 follow-up 2). With no
      // server to return to, the exit lands on the front door in its
      // disconnected state and the ribbon is suppressed (§5.5 degenerate case).
      const summary = url !== null ? lastMatchOf(get().view, get().lobby) : null;
      // Give up the seat: an in-game connection is bridged to its room, so closing
      // the socket is how the seat leaves (the server sends `Leave` on the drop).
      // This also spends the reconnect credential, so a later reload cannot reclaim
      // a game that is over.
      get().disconnect();
      // Drop the terminal view: while it is held, the app renders the game screen no
      // matter what the socket is doing — the dead end of issue #452.
      set({ view: null, spectatorView: null });
      // Return to the lobby by reopening the same server (a fresh `Hello` gets a
      // fresh session and its own first `LobbyView`).
      if (url !== null) get().connect(url, options);
      // After `connect`, which resets the ephemeral per-connection flags: the
      // ribbon deliberately survives this reconnect, because the landing it
      // reports on is on the far side of it.
      if (summary !== null) set({ lastMatch: summary });
    },

    sendLobby(command): void {
      if (!socket) return;
      pendingLobby = pendingKindOf(command);
      // Entering any room supersedes the previous game's ribbon (§5.5's
      // dismissal list). Ephemeral only — nothing else keys off it.
      if (command.type === 'create_room' || command.type === 'join_room') {
        set({ lastMatch: null });
      }
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
      // Tag the submission so the view that answers *this* click can be recognized
      // (issue #554). Purely a correlation handle: it never participates in the
      // content token, and the server only echoes it back in `action_ack`.
      nextSubmission += 1;
      const submission = `s:${nextSubmission}`;
      set({ pendingSubmission: submission });
      socket.send(JSON.stringify(chooseAction(action.id, action.token, targets, submission)));
    },

    setStops(stops, ownTurn): void {
      // Send the seat's stop preferences; the server is authoritative and reflects
      // the accepted sets back in the next `GameView.stops`/`own_turn_stops`. Nothing
      // is stored here — the toggles render from the server's echo, so this survives
      // reconnect. Both halves ride every message, so the seat's whole preference is
      // replaced at once (issue #455) and a cleared default stays cleared.
      if (!socket) return;
      socket.send(JSON.stringify(setStopsMessage(stops, ownTurn)));
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
        //
        // Submission acknowledgement (issue #554): clear the pending marker only when
        // this frame's `action_ack` names the submission still in flight. A view
        // answering someone else's action carries no ack (or another id), so it leaves
        // the marker alone. A lost answer is released by the transport discontinuity
        // that lost it (see the socket lifecycle), never inferred from an ack-less
        // frame — inferring it is exactly the race the correlation removes.
        set((state) => ({
          view: frame.view,
          // A seated game frame supersedes any spectator session.
          spectatorView: null,
          rejectionNonce: frame.view.action_rejected
            ? state.rejectionNonce + 1
            : state.rejectionNonce,
          pendingSubmission: resolvePending(state.pendingSubmission, frame.view),
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
