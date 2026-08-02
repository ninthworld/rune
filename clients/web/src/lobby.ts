/**
 * The lobby, read once: the tables you can reach, and the seats of the one you are at.
 *
 * A `LobbyView` states occupancy, lifecycle, and per-seat flags. Every screen that shows a table
 * needs the same joins over those facts — what a table is called, whether it has room, what a
 * seat has not done yet — and doing them per component is how a directory row and a roster end
 * up disagreeing about the same room. They are done here, once.
 *
 * Nothing here decides anything. Two rules keep it that way:
 *
 * - **A reachable table is one the server advertised a command for.** `valid_commands` is the
 *   only source of interactivity (`docs/protocol.md`), so a row's button exists because
 *   `join_room` or `spectate_room` is currently offered to this connection — never because the
 *   client worked out that a room looked joinable. Occupancy narrows *which* of the two
 *   advertised commands a row leads with; it never invents one that was not offered.
 * - **A seat's status is the flags the server sent.** `decked` and `ready` are stated per seat,
 *   including for an AI seat, and are rendered as stated. What a room needs before it starts is
 *   the server's gate; this module only says which stated flag is still false.
 *
 * The one piece of prose the client owns is a fallback label. The server never invents a name
 * for an unnamed table or an unnamed occupant (a deliberate protocol decision — no prose rides
 * the wire), so the fallback is the client's own display concern and lives here.
 */
import type { LobbyView, RoomState, RoomSummary, RoomView, SeatView } from './protocol'
import { list } from './normalize'

/** Which advertised command reaches a table from the directory. */
export type TableReach = 'join_room' | 'spectate_room'

/** One row of the table directory. */
export interface TableEntry {
  roomId: string
  /** The host's chosen name, falling back to the format id, as `RoomConfig::name` documents. */
  label: string
  /** The opaque `game_setup` identifier, rendered verbatim — the client parses no format id. */
  format: string
  filled: number
  seats: number
  /** Seats with nobody in them. Arithmetic on two stated counts, not a legality claim. */
  open: number
  spectators: number
  state: RoomState
  stateLabel: string
  /** The command this row offers, or `undefined` when the server advertised neither. */
  reach?: TableReach
}

const STATE_LABELS: Record<RoomState, string> = {
  gathering: 'Gathering',
  in_progress: 'In progress',
}

/**
 * The label for a table with no name of its own.
 *
 * `RoomConfig::name` is optional and the server never fills it in, so an unnamed table is
 * labelled by its format exactly as it was before the field existed.
 */
export const tableLabel = (summary: Pick<RoomSummary, 'config'>): string =>
  summary.config.name ?? summary.config.game_setup

/**
 * The directory as rows, in the order the server listed them.
 *
 * `commands` is the connection's `valid_commands`. A room with an open seat leads with `Join`
 * when the server is offering `join_room`; anything else leads with `Watch` when it is offering
 * `spectate_room` — a full or already-started table is still worth reaching, and spectating
 * consumes no seat. A connection already in a room is offered neither, and gets rows that only
 * inform.
 */
export function tables(view: LobbyView): readonly TableEntry[] {
  const commands = list(view.valid_commands)
  return list(view.directory).map((summary) => {
    const seats = summary.config.seats
    const open = Math.max(0, seats - summary.filled)
    const joinable = summary.state === 'gathering' && open > 0
    return {
      roomId: summary.room_id,
      label: tableLabel(summary),
      format: summary.config.game_setup,
      filled: summary.filled,
      seats,
      open,
      spectators: summary.spectators ?? 0,
      state: summary.state,
      stateLabel: STATE_LABELS[summary.state],
      reach:
        joinable && commands.includes('join_room')
          ? 'join_room'
          : commands.includes('spectate_room')
            ? 'spectate_room'
            : undefined,
    }
  })
}

/**
 * One thing a seat has or has not done, as a mark on that seat.
 *
 * The marks are `docs/client-design.md` §9.2's second rule made concrete: what a table is
 * waiting on is drawn **on the seat it belongs to**, as a mark that is either met or not, rather
 * than restated underneath as a sentence about seats the reader can already see. A mark's word
 * does not change with its state — `Deck` lit and `Deck` unlit are the same fact answered two
 * ways — because a label that rewrote itself would have to be read rather than scanned.
 */
export interface SeatMark {
  /** The word, drawn. */
  label: string
  /** Whether the server stated this flag true. */
  met: boolean
  /** The whole fact in words, for assistive technology, which cannot perceive lit or unlit. */
  detail: string
}

/** One seat of the room this connection is in. */
export interface SeatRow {
  seat: number
  /** The occupant's chosen name, their opaque id, the AI's advertised name, or the seat itself. */
  label: string
  occupied: boolean
  you: boolean
  /** The AI kind occupying this seat (`SeatView::ai`), if one does. */
  ai?: string
  decked: boolean
  ready: boolean
  /** What this seat still owes, as marks. Empty for a seat nobody is in: it owes nothing yet. */
  marks: readonly SeatMark[]
  /** The first stated flag that is still false, or `undefined` when the seat is set. */
  awaiting?: string
}

/**
 * The room's seats, in seat order.
 *
 * `aiNames` maps an AI kind id to the display name the catalog advertised (`AiOption::name`), so
 * a seated bot reads as what the server calls it rather than as a raw id. Absent for a client
 * that has not fetched the catalog, in which case the seat's own `name` — or the kind id — does.
 */
export function roster(
  room: RoomView,
  you: string | undefined,
  aiNames: Readonly<Record<string, string>> = {},
): readonly SeatRow[] {
  return list(room.seats).map((seat) => seatRow(seat, you, aiNames))
}

function seatRow(
  seat: SeatView,
  you: string | undefined,
  aiNames: Readonly<Record<string, string>>,
): SeatRow {
  const occupied = seat.occupied_by !== undefined || seat.ai !== undefined
  const decked = seat.decked === true
  const ready = seat.ready === true
  // A seat nobody is in owes neither of these: what it is waiting on is somebody, and that is
  // the seat itself rather than a mark on it.
  const marks: SeatMark[] = occupied
    ? [
        { label: 'Deck', met: decked, detail: decked ? 'Deck submitted' : 'No deck yet' },
        { label: 'Ready', met: ready, detail: ready ? 'Ready' : 'Not ready' },
      ]
    : []

  return {
    seat: seat.seat,
    label:
      seat.name ??
      (seat.ai !== undefined ? (aiNames[seat.ai] ?? seat.ai) : undefined) ??
      seat.occupied_by ??
      `Seat ${seat.seat + 1}`,
    occupied,
    you: you !== undefined && seat.occupied_by === you,
    ai: seat.ai,
    decked,
    ready,
    marks,
    awaiting: !occupied
      ? 'Nobody here yet'
      : !decked
        ? 'No deck yet'
        : !ready
          ? 'Not ready'
          : undefined,
  }
}
