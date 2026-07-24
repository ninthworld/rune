/**
 * The room directory (issue #506; `front-door-and-lobby.md` §5.2) — the PRIMARY
 * "find a game" path, leading the Lobby place's left column.
 *
 * Each row is a lift-on-focus tile carrying the setup label, `filled/total` with
 * a **seat-pip row** (one pip per seat, filled pips in that seat's accent, empty
 * pips as dashed outlines) so occupancy reads as a shape and not only a number,
 * the spectator count when > 0, and the row's action — Join, Full, or In
 * progress + Spectate, **exactly as advertised by `valid_commands`**. No
 * legality is computed here.
 *
 * The empty state (fixes P5) is a full-width invitation that *contains* the
 * create affordance rather than pointing "below" at one.
 *
 * Before the first `LobbyView` (fixes P8) the same composition renders with a
 * skeleton of placeholder rows, so the shape a player is waiting for is the
 * shape they see.
 */
import { cx } from '../chrome/cx';
import type { LobbyView, RoomSummary } from '../protocol';
import { joinRoomCommand, spectateRoomCommand } from '../protocol';
import { useGameStore } from '../store';
import { seatAccentVars } from './pregameScene';
import { setupLabel } from './gameSetups';
import p from './styles';

/** Whether a command kind is currently offered to this connection. */
function can(view: LobbyView, command: string): boolean {
  return view.valid_commands.includes(command);
}

/**
 * Occupancy as a shape: one pip per seat, the filled ones wearing the accent of
 * the seat they stand for. Indicator-class, and it never carries text — the
 * `n/m` number beside it is the reading channel (§5.9).
 */
function SeatPips({ filled, total, roomId }: { filled: number; total: number; roomId: string }) {
  return (
    <span className={p.pipRow} aria-hidden="true" data-testid={`room-${roomId}-pips`}>
      {Array.from({ length: total }, (_, index) => (
        <span
          key={index}
          className={cx(p.pip, index < filled && p.pipFilled)}
          style={index < filled ? seatAccentVars(index) : undefined}
        />
      ))}
    </span>
  );
}

/** One directory row. Everything is derived from the `RoomSummary`. */
export function RoomDirectoryRow({
  room,
  canJoin,
  canSpectate,
  onJoin,
  onSpectate,
}: {
  room: RoomSummary;
  canJoin: boolean;
  canSpectate: boolean;
  onJoin: (roomId: string) => void;
  onSpectate: (roomId: string) => void;
}) {
  const total = room.config.seats;
  const started = room.state === 'in_progress';
  const full = room.filled >= total;

  // A started room is un-joinable but **spectatable** (issue #351) when the
  // server offers `spectate_room`; an open gathering room offers Join (gated by
  // `join_room`); a full one shows Full.
  const action = started ? (
    <>
      <span className={p.badge} data-testid={`room-${room.room_id}-in-progress`}>
        In progress
      </span>
      {canSpectate && (
        <button
          type="button"
          className={p.button}
          onClick={() => onSpectate(room.room_id)}
          data-testid={`spectate-directory-${room.room_id}`}
        >
          Spectate
        </button>
      )}
    </>
  ) : full ? (
    <span className={p.badge} data-testid={`room-${room.room_id}-full`}>
      Full
    </span>
  ) : canJoin ? (
    <button
      type="button"
      className={p.button}
      onClick={() => onJoin(room.room_id)}
      data-testid={`join-directory-${room.room_id}`}
    >
      Join
    </button>
  ) : null;

  return (
    <li className={p.directoryRow} data-testid={`room-row-${room.room_id}`}>
      <span className={p.directoryInfo}>
        <span className={p.directoryName}>{setupLabel(room.config.game_setup)}</span>
        <span className={p.directoryMeta}>
          <SeatPips filled={room.filled} total={total} roomId={room.room_id} />
          <span data-testid={`room-${room.room_id}-occupancy`}>
            {room.filled}/{total} filled
            {room.spectators > 0 && (
              <span data-testid={`room-${room.room_id}-spectators`}>
                {' '}
                · {room.spectators} watching
              </span>
            )}
          </span>
        </span>
      </span>
      <span className={p.directoryActions}>{action}</span>
    </li>
  );
}

/** The three placeholder rows shown before the first `LobbyView` arrives. */
export function DirectorySkeleton() {
  return (
    <div className={p.directoryList} data-testid="room-directory-skeleton" aria-hidden="true">
      {[0, 1, 2].map((row) => (
        <div key={row} className={p.skeletonRow} />
      ))}
    </div>
  );
}

export interface RoomDirectoryProps {
  /** The latest view, or `undefined` before the first frame (skeleton state). */
  view?: LobbyView;
  /** Focus the Start-a-game card in Create mode (the empty state's action). */
  onCreateHere: () => void;
}

export function RoomDirectory({ view, onCreateHere }: RoomDirectoryProps) {
  const sendLobby = useGameStore((state) => state.sendLobby);
  const canJoin = view !== undefined && can(view, 'join_room');
  const canSpectate = view !== undefined && can(view, 'spectate_room');
  const canCreate = view !== undefined && can(view, 'create_room');

  return (
    <section className={p.panel} aria-label="Open games" data-testid="room-directory">
      <span className={p.kicker}>Find a game</span>
      <h2 className={p.title} data-place-heading tabIndex={-1}>
        Open games
      </h2>
      {view === undefined ? (
        <>
          <span className={p.muted} data-testid="room-directory-loading">
            Loading open games…
          </span>
          <DirectorySkeleton />
        </>
      ) : view.directory.length === 0 ? (
        // The empty state CONTAINS the create action (fixes P5).
        <div className={p.emptyDirectory} data-testid="room-directory-empty">
          <span>No open games right now — start one and share the id.</span>
          {canCreate && (
            <button
              type="button"
              className={p.button}
              onClick={onCreateHere}
              data-testid="room-directory-empty-create"
            >
              Create a room
            </button>
          )}
        </div>
      ) : (
        <ul className={p.directoryList} data-testid="room-directory-list">
          {view.directory.map((room) => (
            <RoomDirectoryRow
              key={room.room_id}
              room={room}
              canJoin={canJoin}
              canSpectate={canSpectate}
              onJoin={(roomId) => sendLobby(joinRoomCommand(roomId))}
              onSpectate={(roomId) => sendLobby(spectateRoomCommand(roomId))}
            />
          ))}
        </ul>
      )}
    </section>
  );
}
