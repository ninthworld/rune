//! What the lobby refuses, and how each refusal reads to a player.

use crate::lobby::*;

/// Why a [`LobbyCommand`] was rejected. On any of these the connection's current
/// [`LobbyView`] is re-sent unchanged; the typed value lets the server
/// (and tests) distinguish, e.g., a full room from an unknown one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LobbyError {
    /// The command came from a session the registry does not know.
    UnknownSession,
    /// `create_room`/`join_room` while already seated in a room.
    AlreadyInRoom,
    /// A command that requires being in a room (e.g. `leave`) with no room.
    NotInRoom,
    /// `create_room` with a seat count outside [`SEAT_RANGE`].
    InvalidSeatCount(u8),
    /// `create_room` whose seat count is valid for the lobby but outside the chosen
    /// format's own seat range (issue #349): e.g. 4 seats for a two-player format, or
    /// 2 for a free-for-all. Carries the seat count and the format id; no room opens.
    SeatCountForFormat {
        /// The requested seat count.
        seats: u8,
        /// The format id it was rejected for.
        format: String,
    },
    /// `create_room` whose `game_setup` id names no format in the registry (ADR
    /// 0013 §4). Carries the offending id; no room is opened.
    UnknownFormat(String),
    /// `update_room` (issue #546) whose new seat count would remove an **occupied**
    /// seat. Shrinking a table is rejected outright rather than silently clamped or
    /// evicting anybody: carries the requested count and the number of seats the room
    /// must keep (one past its highest occupied seat). The config is left untouched.
    SeatsBelowOccupancy {
        /// The requested seat count.
        seats: u8,
        /// The smallest seat count that keeps every current occupant seated.
        needed: u8,
    },
    /// `create_room`/`update_room` whose table name failed validation (issue #546).
    /// A table name obeys the same bounds a [`SetName`] display name does; carries the
    /// specific [`NameError`]. The room's config is left untouched.
    InvalidRoomName(NameError),
    /// `join_room` with an id no active room has.
    UnknownRoom,
    /// `join_room` on a room whose every seat is occupied.
    RoomFull,
    /// `spectate_room` on a room whose game has not started yet (issue
    /// #351): there is no live board to watch until the ready gate passes. The client
    /// may retry once the room shows [`RoomState::InProgress`] in the directory.
    RoomNotStarted,
    /// `create_room` while the registry is already at [`Lobby::max_rooms`].
    AtCapacity,
    /// A `submit_deck`/`ready` command from a session that is not seated in a room.
    NotSeated,
    /// `submit_deck` whose decklist held a card identity that does not resolve to a
    /// known card in the database. Carries the offending identity; the seat stays
    /// undecked (its previous deck, if any, is untouched).
    UnknownCard(String),
    /// `submit_deck` whose decklist is illegal for the room's format:
    /// too few or too many cards, or too many copies of a non-basic card. Carries a
    /// [`DeckError`] naming the violation; the seat keeps whatever deck it had. The
    /// structured reason is delivered to the rejecting seat alone as a
    /// [`LobbyErrorFrame`](sage_protocol::LobbyErrorFrame) (issue #395, see
    /// [`Lobby::deck_rejection`]).
    IllegalDeck(DeckError),
    /// `ready` (up) on a seat that has not yet submitted a valid deck.
    NotDecked,
    /// A lobby command aimed at a room whose game has already started (its seats
    /// speak `GameView`s now, not lobby commands).
    GameStarted,
    /// A host-only command (`add_ai`/`remove_ai`, issue #415; `update_room`, issue
    /// #546) from a session that is not the room's **host** — the seat 0 occupant. Only
    /// the host manages AI seats and the table's configuration; a non-host request is a
    /// non-fatal rejection that changes nothing.
    NotHost,
    /// `add_ai`/`remove_ai` naming a seat index outside the room's seat range. Carries the
    /// offending index.
    SeatIndexOutOfRange(u8),
    /// `add_ai` targeting a seat that is already occupied (by a human or an AI, issue
    /// #415). AI opponents fill only empty seats.
    SeatOccupied(u8),
    /// `add_ai` whose `kind` names no AI the server supports (issue #415). Carries the
    /// offending kind id; the seat is untouched.
    UnknownAiKind(String),
    /// `remove_ai` on a seat that holds no AI opponent (issue #415). Carries the seat
    /// index.
    NotAiSeat(u8),
    /// `set_name` whose requested display name failed validation (issue #294). Carries
    /// the specific [`NameError`]; the connection keeps whatever name it had and its
    /// current [`LobbyView`] is re-sent unchanged (the non-fatal pattern).
    InvalidName(NameError),
}

/// Why a requested display name was rejected (issue #294). A closed enum so a new
/// validation rule forces a matching arm rather than a silent catch-all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NameError {
    /// The name was empty (or only whitespace) after trimming.
    Empty,
    /// The name exceeded [`MAX_NAME_LEN`] scalar values. Carries the trimmed length.
    TooLong(usize),
    /// The name held a control character (e.g. a newline or NUL) — display names must
    /// be printable text.
    Unprintable,
}

impl std::fmt::Display for NameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => write!(f, "display name is empty"),
            Self::TooLong(len) => {
                write!(
                    f,
                    "display name is {len} characters, over the {MAX_NAME_LEN} limit"
                )
            }
            Self::Unprintable => write!(f, "display name has a non-printable character"),
        }
    }
}

impl std::fmt::Display for LobbyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownSession => write!(f, "unknown session"),
            Self::AlreadyInRoom => write!(f, "already in a room"),
            Self::NotInRoom => write!(f, "not in a room"),
            Self::InvalidSeatCount(n) => write!(f, "seat count {n} is outside 2..=8"),
            Self::SeatCountForFormat { seats, format } => {
                write!(f, "seat count {seats} is not allowed by format {format}")
            }
            Self::UnknownFormat(id) => write!(f, "unknown game_setup format {id}"),
            Self::SeatsBelowOccupancy { seats, needed } => write!(
                f,
                "seat count {seats} would remove an occupied seat; {needed} seats are in use"
            ),
            Self::InvalidRoomName(error) => write!(f, "invalid table name: {error}"),
            Self::UnknownRoom => write!(f, "unknown room id"),
            Self::RoomFull => write!(f, "room is full"),
            Self::RoomNotStarted => write!(f, "room's game has not started yet"),
            Self::AtCapacity => write!(f, "lobby is at room capacity"),
            Self::NotSeated => write!(f, "not seated in a room"),
            Self::UnknownCard(id) => write!(f, "unknown card identity {id}"),
            Self::IllegalDeck(error) => write!(f, "illegal deck: {error}"),
            Self::NotDecked => write!(f, "seat has not submitted a valid deck"),
            Self::GameStarted => write!(f, "the room's game has already started"),
            Self::NotHost => write!(
                f,
                "only the room host may manage AI seats and the table configuration"
            ),
            Self::SeatIndexOutOfRange(seat) => write!(f, "seat index {seat} is out of range"),
            Self::SeatOccupied(seat) => write!(f, "seat {seat} is already occupied"),
            Self::UnknownAiKind(kind) => write!(f, "unknown AI kind {kind}"),
            Self::NotAiSeat(seat) => write!(f, "seat {seat} holds no AI opponent"),
            Self::InvalidName(error) => write!(f, "invalid display name: {error}"),
        }
    }
}

impl std::error::Error for LobbyError {}
