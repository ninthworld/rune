/**
 * Open games — the focus of the connected server lobby (issue #546; the approved
 * `docs/ui-concepts/rune-pregame-server-lobby.jpg` baseline).
 *
 * The baseline's rule is that **selecting a table is what makes Join the natural
 * primary action**. So the list is a list of *surfaces*, not of controls: one
 * plaque per table carrying its name, format, and occupancy, selected one at a
 * time. There is exactly one control on the screen it leads to — the blue
 * primary the lobby renders below — instead of a Join button on every row
 * competing with it.
 *
 * Nothing here computes legality. Whether the selected table can be joined or
 * watched is the lobby's read of `valid_commands` plus the row's own
 * `RoomSummary.state`, and the row renders the summary and no more.
 *
 * Before the first `LobbyView` the same composition renders with placeholder
 * rows, so the shape a player is waiting for is the shape they see.
 */
import { cx } from '../chrome/cx';
import type { LobbyView, RoomSummary } from '../protocol';
import { Plaque } from './MenuFrame';
import { setupLabel, tableName } from './gameSetups';
import { seatAccentVars } from './pregameScene';
import p from './styles';

/**
 * Occupancy as a shape: one pip per seat, filled pips wearing the accent of the
 * seat they stand for. Indicator-class — it never carries text, because the
 * `n / m` beside it is the reading channel.
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

/**
 * One table. A started room is shown for context and says so in words; whether
 * it can be watched is the lobby's decision, from `valid_commands`.
 */
export function RoomDirectoryRow({
  room,
  selected,
  onSelect,
}: {
  room: RoomSummary;
  selected: boolean;
  onSelect: (roomId: string) => void;
}) {
  const total = room.config.seats;
  const started = room.state === 'in_progress';
  const full = room.filled >= total;
  // The row's second line stays the table's *state* where it has one to report, and
  // otherwise names the format — which is what the host's chosen name (issue #546)
  // is now free to stop doing on the first line.
  const state = started ? 'In progress' : full ? 'Full' : setupLabel(room.config.game_setup);

  return (
    <li>
      <button
        type="button"
        className={p.gameRow}
        aria-pressed={selected}
        onClick={() => onSelect(room.room_id)}
        data-testid={`room-row-${room.room_id}`}
      >
        <Plaque selected={selected} faceClass={p.gameRowFace}>
          {/* The non-colour half of selection: a hollow diamond fills in. */}
          <span className={cx(p.rowPip, selected && p.rowPipOn)} aria-hidden="true" />
          <span className={p.rowName}>{tableName(room.config)}</span>
          <span
            className={p.rowMeta}
            data-testid={started ? `room-${room.room_id}-in-progress` : undefined}
          >
            {state}
          </span>
          <span className={p.rowCount} data-testid={`room-${room.room_id}-occupancy`}>
            {room.filled}/{total} filled
            {room.spectators > 0 && (
              <span data-testid={`room-${room.room_id}-spectators`}>
                {' '}
                · {room.spectators} watching
              </span>
            )}
          </span>
          <SeatPips filled={room.filled} total={total} roomId={room.room_id} />
        </Plaque>
      </button>
    </li>
  );
}

/** The placeholder rows shown before the first `LobbyView` arrives. */
export function DirectorySkeleton() {
  return (
    <div className={p.gameList} data-testid="room-directory-skeleton" aria-hidden="true">
      {[0, 1, 2].map((row) => (
        <div key={row} className={p.skeletonRow} />
      ))}
    </div>
  );
}

export interface RoomDirectoryProps {
  /** The latest view, or `undefined` before the first frame (skeleton state). */
  view?: LobbyView;
  /** The currently selected table, if any. */
  selectedId?: string;
  /** Select a table (which is what promotes Join/Watch to the primary). */
  onSelect: (roomId: string) => void;
}

export function RoomDirectory({ view, selectedId, onSelect }: RoomDirectoryProps) {
  return (
    <div className={p.controlColumn} data-testid="room-directory">
      <h2 className={p.arenaTitle} data-place-heading tabIndex={-1}>
        Open Games
      </h2>
      {view === undefined ? (
        <>
          <span className={p.muted} data-testid="room-directory-loading">
            Loading open games…
          </span>
          <DirectorySkeleton />
        </>
      ) : view.directory.length === 0 ? (
        <span className={p.emptyGames} data-testid="room-directory-empty">
          No open games right now — create one and share its id.
        </span>
      ) : (
        <ul className={p.gameList} data-testid="room-directory-list">
          {view.directory.map((room) => (
            <RoomDirectoryRow
              key={room.room_id}
              room={room}
              selected={room.room_id === selectedId}
              onSelect={onSelect}
            />
          ))}
        </ul>
      )}
    </div>
  );
}
