/**
 * The pre-game lobby screen (issue #114).
 *
 * The screen shown between the {@link ConnectionScreen} and the in-game
 * {@link Table}: after the socket opens, the store greets the server (`Hello`)
 * and this screen renders the resulting {@link LobbyView} — create a room (with a
 * seat count), join one by id, pick a bundled starter deck, submit it, and ready
 * up. When every seat is filled, decked, and ready the server constructs the game
 * and pushes the first `GameView`; the app then switches to the table.
 *
 * Hard rules (AGENTS.md, ADR 0012):
 * - **Reconstruct from one `LobbyView`.** Every control here is derived from the
 *   store's latest `LobbyView`; nothing about the lobby is load-bearing across
 *   messages. Local component state is ephemeral form input only (the seat count
 *   being typed, the deck picked in the dropdown, a "Copied" flash).
 * - **`valid_commands` is the only source of interactivity.** A create/join/deck/
 *   ready/leave affordance is offered only when the server advertised that command
 *   for this connection; the client computes no legality.
 * - **No card logic.** The bundled decklists are static names/ids (see
 *   `decklists.ts`); the server validates a submitted deck authoritatively.
 * - **Never a dead screen.** Before the first `LobbyView`, and on every error, an
 *   interactive control (Disconnect / retry-able form) is always on screen.
 */
import { useEffect, useState } from 'react';
import { STARTER_DECKLISTS, decklistCards, decklistById, decklistSize } from './decklists';
import {
  createRoomCommand,
  joinRoomCommand,
  leaveCommand,
  readyCommand,
  submitDeckCommand,
  type LobbyView,
  type RoomSummary,
  type SeatView,
} from './protocol';
import { useGameStore } from './store';
import { cx } from './chrome/cx';
import s from './table/chrome.module.css';

/** A game-setup option offered by the create-room form. */
interface GameSetupOption {
  /** The opaque `game_setup` id sent to the server (see ADR 0013 vocabulary). */
  readonly id: string;
  /** Display label. */
  readonly label: string;
  /** The seat count this setup is designed for (pre-fills the seat selector). */
  readonly seats: number;
}

/**
 * Game-setup options offered in the create form. The `game_setup` id is opaque to
 * the client and validated server-side; these are just the choices a player picks
 * from (the catalogue is owned by ADR 0013 / the server's format registry).
 */
const GAME_SETUPS: readonly GameSetupOption[] = [
  { id: '1v1', label: '1v1 Duel', seats: 2 },
  { id: 'ffa-4', label: 'Free-for-all (4)', seats: 4 },
];

/** The seat counts the lobby offers, matching the protocol's `2..=8` range. */
const SEAT_COUNTS = [2, 3, 4, 5, 6, 7, 8] as const;

/** Whether a command kind is currently offered to this connection. */
function can(view: LobbyView, command: string): boolean {
  return view.valid_commands.includes(command);
}

/**
 * A human label for an opaque `game_setup` id: the known options' display label,
 * falling back to the raw id (which is server-owned and forward-compatible — an
 * unknown setup still renders, never blank).
 */
function setupLabel(gameSetup: string): string {
  return GAME_SETUPS.find((option) => option.id === gameSetup)?.label ?? gameSetup;
}

/** Human label for a seat's occupant. */
function occupantLabel(seat: SeatView, you: string): string {
  if (seat.occupied_by === undefined) return 'Open';
  if (seat.occupied_by === you) return 'You';
  return seat.occupied_by;
}

/** The pre-first-frame lobby fallback: a live status plus a Disconnect action. */
function LobbyWaiting({ onDisconnect }: { onDisconnect: () => void }) {
  return (
    <main className={s.connectMain}>
      <section className={s.lobbyPanel} aria-label="Entering lobby" data-testid="lobby-waiting">
        <h1 className={s.connectHeading}>RUNE</h1>
        <div className={s.waitingBar}>
          <span className={s.muted}>Connected — entering the lobby…</span>
          <button
            type="button"
            className={s.button}
            onClick={onDisconnect}
            data-testid="lobby-disconnect-button"
          >
            Disconnect
          </button>
        </div>
      </section>
    </main>
  );
}

/**
 * One row of the room directory (issue #280). A `gathering` room with an open seat
 * shows a Join button (only when `join_room` is offered — `valid_commands` gates
 * interactivity); a full `gathering` room shows "Full"; an `in_progress` room is
 * visible but un-joinable. All of it is derived from the `RoomSummary` — no legality
 * computed here.
 */
function RoomDirectoryRow({
  room,
  canJoin,
  onJoin,
}: {
  room: RoomSummary;
  canJoin: boolean;
  onJoin: (roomId: string) => void;
}) {
  const total = room.config.seats;
  const started = room.state === 'in_progress';
  const full = room.filled >= total;

  // Priority: a started room is un-joinable; a full gathering room is Full; an open
  // gathering room offers Join only when the server advertised `join_room`.
  const action = started ? (
    <span className={s.seatBadge} data-testid={`room-${room.room_id}-in-progress`}>
      In progress
    </span>
  ) : full ? (
    <span className={s.seatBadge} data-testid={`room-${room.room_id}-full`}>
      Full
    </span>
  ) : canJoin ? (
    <button
      type="button"
      className={s.button}
      onClick={() => onJoin(room.room_id)}
      data-testid={`join-directory-${room.room_id}`}
    >
      Join
    </button>
  ) : null;

  return (
    <li className={s.roomRow} data-testid={`room-row-${room.room_id}`}>
      <span className={s.roomRowInfo}>
        <span>
          {setupLabel(room.config.game_setup)} · {total} seats
        </span>
        <span className={s.muted} data-testid={`room-${room.room_id}-occupancy`}>
          {room.filled}/{total} filled
        </span>
      </span>
      <span className={s.roomRowActions}>{action}</span>
    </li>
  );
}

/** The room browser (issue #280): the list of open games, plus an empty state. */
function RoomDirectory({ view }: { view: LobbyView }) {
  const sendLobby = useGameStore((state) => state.sendLobby);
  const canJoin = can(view, 'join_room');
  const join = (roomId: string): void => {
    sendLobby(joinRoomCommand(roomId));
  };

  return (
    <section className={s.lobbySection} aria-label="Open games" data-testid="room-directory">
      <h2 className={s.lobbySectionTitle}>Open games</h2>
      {view.directory.length === 0 ? (
        <span className={s.roomListEmpty} data-testid="room-directory-empty">
          No open games — create one.
        </span>
      ) : (
        <ul className={s.roomList} data-testid="room-directory-list">
          {view.directory.map((room) => (
            <RoomDirectoryRow key={room.room_id} room={room} canJoin={canJoin} onJoin={join} />
          ))}
        </ul>
      )}
    </section>
  );
}

/** The create-a-room / room-directory / join-by-id screen, shown when room-less. */
function RoomEntry({ view }: { view: LobbyView }) {
  const sendLobby = useGameStore((state) => state.sendLobby);
  const [setupId, setSetupId] = useState(GAME_SETUPS[0].id);
  const [seats, setSeats] = useState<number>(GAME_SETUPS[0].seats);
  const [roomId, setRoomId] = useState('');
  const [joinError, setJoinError] = useState<string | null>(null);

  const create = (): void => {
    sendLobby(createRoomCommand({ seats, game_setup: setupId }));
  };

  const join = (): void => {
    const target = roomId.trim();
    if (target.length === 0) {
      setJoinError('Enter a room id to join.');
      return;
    }
    setJoinError(null);
    sendLobby(joinRoomCommand(target));
  };

  return (
    <>
      {can(view, 'create_room') && (
        <section className={s.lobbySection} aria-label="Create a room" data-testid="create-room">
          <h2 className={s.lobbySectionTitle}>Create a room</h2>
          <label className={s.field}>
            <span className={s.fieldLabel}>Game setup</span>
            <select
              className={s.select}
              value={setupId}
              data-testid="game-setup-select"
              onChange={(event) => {
                const next = event.target.value;
                setSetupId(next);
                const found = GAME_SETUPS.find((option) => option.id === next);
                if (found) setSeats(found.seats);
              }}
            >
              {GAME_SETUPS.map((option) => (
                <option key={option.id} value={option.id}>
                  {option.label}
                </option>
              ))}
            </select>
          </label>
          <label className={s.field}>
            <span className={s.fieldLabel}>Seats</span>
            <select
              className={s.select}
              value={seats}
              data-testid="seat-count-select"
              onChange={(event) => setSeats(Number(event.target.value))}
            >
              {SEAT_COUNTS.map((count) => (
                <option key={count} value={count}>
                  {count}
                </option>
              ))}
            </select>
          </label>
          <div className={s.buttonRow}>
            <button
              type="button"
              className={s.button}
              onClick={create}
              data-testid="create-room-button"
            >
              Create room
            </button>
          </div>
        </section>
      )}

      <RoomDirectory view={view} />

      {can(view, 'join_room') && (
        <details className={s.lobbySection} data-testid="join-room">
          <summary className={s.joinByIdSummary}>Join by room id</summary>
          <label className={s.field}>
            <span className={s.fieldLabel}>Room id</span>
            <input
              className={s.select}
              type="text"
              autoComplete="off"
              spellCheck={false}
              value={roomId}
              onChange={(event) => setRoomId(event.target.value)}
              data-testid="join-room-input"
              aria-label="Room id"
            />
          </label>
          {joinError !== null && (
            <span className={s.errorText} role="alert" data-testid="join-room-error">
              {joinError}
            </span>
          )}
          <div className={s.buttonRow}>
            <button
              type="button"
              className={s.button}
              onClick={join}
              data-testid="join-room-button"
            >
              Join room
            </button>
          </div>
        </details>
      )}
    </>
  );
}

/** The room roster + deck/ready controls, shown once in a room. */
function RoomPanel({ view }: { view: LobbyView }) {
  const sendLobby = useGameStore((state) => state.sendLobby);
  const room = view.room;
  const [deckId, setDeckId] = useState(STARTER_DECKLISTS[0].id);
  const [copied, setCopied] = useState(false);

  // The "Copied" flash is transient chrome; clear it shortly after it shows so the
  // button returns to its idle label (nothing load-bearing).
  useEffect(() => {
    if (!copied) return;
    const timer = setTimeout(() => setCopied(false), 1500);
    return () => clearTimeout(timer);
  }, [copied]);

  if (room === undefined) return null;
  const mySeat = room.seats.find((seat) => seat.occupied_by === view.you);

  const copyRoomId = (): void => {
    const write = navigator.clipboard?.writeText?.(room.room_id);
    if (write && typeof write.then === 'function') {
      write.then(
        () => setCopied(true),
        () => setCopied(false),
      );
    } else {
      setCopied(true);
    }
  };

  const submitDeck = (): void => {
    const deck = decklistById(deckId);
    if (deck === undefined) return;
    sendLobby(submitDeckCommand(decklistCards(deck)));
  };

  return (
    <>
      <section className={s.lobbySection} aria-label="Room" data-testid="room-panel">
        <h2 className={s.lobbySectionTitle}>Room</h2>
        <div className={s.roomIdRow}>
          <span className={s.fieldLabel}>Room id</span>
          <code className={s.roomIdCode} data-testid="room-id">
            {room.room_id}
          </code>
          <button
            type="button"
            className={s.button}
            onClick={copyRoomId}
            data-testid="copy-room-id-button"
            aria-label="Copy room id"
          >
            {copied ? 'Copied' : 'Copy'}
          </button>
        </div>
        <span className={s.muted}>Share this id so another player can join.</span>

        <ul className={s.seatList} data-testid="seat-list">
          {room.seats.map((seat) => {
            const isLocal = seat.occupied_by !== undefined && seat.occupied_by === view.you;
            return (
              <li
                key={seat.seat}
                className={isLocal ? cx(s.seatRow, s.seatRowLocal) : s.seatRow}
                data-testid={`seat-${seat.seat}`}
              >
                <span>Seat {seat.seat + 1}</span>
                <span className={s.muted}>{occupantLabel(seat, view.you)}</span>
                <span className={s.seatBadges}>
                  <span className={seat.occupied_by !== undefined ? s.seatBadgeOn : s.seatBadge}>
                    {seat.occupied_by !== undefined ? 'Filled' : 'Open'}
                  </span>
                  {seat.decked === true && (
                    <span className={s.seatBadgeOn} data-testid={`seat-${seat.seat}-decked`}>
                      Decked
                    </span>
                  )}
                  {seat.ready === true && (
                    <span className={s.seatBadgeOn} data-testid={`seat-${seat.seat}-ready`}>
                      Ready
                    </span>
                  )}
                </span>
              </li>
            );
          })}
        </ul>
      </section>

      {can(view, 'submit_deck') && (
        <section
          className={s.lobbySection}
          aria-label="Choose a deck"
          data-testid="deck-select-section"
        >
          <h2 className={s.lobbySectionTitle}>Choose a deck</h2>
          <label className={s.field}>
            <span className={s.fieldLabel}>Starter deck</span>
            <select
              className={s.select}
              value={deckId}
              data-testid="deck-select"
              onChange={(event) => setDeckId(event.target.value)}
            >
              {STARTER_DECKLISTS.map((deck) => (
                <option key={deck.id} value={deck.id}>
                  {deck.name} ({decklistSize(deck)} cards)
                </option>
              ))}
            </select>
          </label>
          <span className={s.muted}>{decklistById(deckId)?.summary}</span>
          <div className={s.buttonRow}>
            <button
              type="button"
              className={s.button}
              onClick={submitDeck}
              data-testid="submit-deck-button"
            >
              {mySeat?.decked === true ? 'Resubmit deck' : 'Submit deck'}
            </button>
          </div>
        </section>
      )}

      <div className={s.buttonRow}>
        {can(view, 'ready') && (
          <button
            type="button"
            className={s.button}
            onClick={() => sendLobby(readyCommand(true))}
            data-testid="ready-button"
          >
            Ready
          </button>
        )}
        {can(view, 'unready') && (
          <button
            type="button"
            className={s.button}
            onClick={() => sendLobby(readyCommand(false))}
            data-testid="unready-button"
          >
            Not ready
          </button>
        )}
        {can(view, 'leave') && (
          <button
            type="button"
            className={s.button}
            onClick={() => sendLobby(leaveCommand())}
            data-testid="leave-room-button"
          >
            Leave room
          </button>
        )}
      </div>
    </>
  );
}

export function LobbyScreen() {
  const lobby = useGameStore((state) => state.lobby);
  const lobbyError = useGameStore((state) => state.lobbyError);
  const disconnect = useGameStore((state) => state.disconnect);

  // Socket is open but the first LobbyView has not arrived yet: keep an
  // interactive Disconnect on screen (never a dead screen).
  if (lobby === null) {
    return <LobbyWaiting onDisconnect={disconnect} />;
  }

  return (
    <main className={s.connectMain}>
      <section className={s.lobbyPanel} aria-label="Lobby" data-testid="lobby-screen">
        <h1 className={s.connectHeading}>RUNE Lobby</h1>
        {lobbyError !== null && (
          <span className={s.errorText} role="alert" data-testid="lobby-error">
            {lobbyError}
          </span>
        )}
        {lobby.room === undefined ? <RoomEntry view={lobby} /> : <RoomPanel view={lobby} />}
        <div className={s.buttonRow}>
          <button
            type="button"
            className={s.button}
            onClick={disconnect}
            data-testid="lobby-disconnect-button"
          >
            Disconnect
          </button>
        </div>
      </section>
    </main>
  );
}
