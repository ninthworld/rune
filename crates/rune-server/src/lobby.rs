//! Layer 1 lobby — session identity, the explicit-room registry, the pre-game
//! `LobbyView`/`LobbyCommand` routing (ADR 0012, issue #110), the deck-submission and
//! ready gate that constructs the game and hands each seat off to the in-game contract
//! (issue #112), and reconnect-to-a-held-seat by session token (issue #113).
//!
//! The lobby is the connective tissue between the accept loop (issue #30) and the
//! room task (issue #31). It owns the **room registry** — the shared
//! `Arc<RwLock<...>>` of active rooms from `docs/brief.md` — and the set of live
//! **sessions**. A new connection lands here in the pre-game phase: it is issued an
//! opaque session token and a [`LobbyView`], and it drives itself with
//! [`LobbyCommand`]s ([`serve_lobby_connection`]). The lobby never reads or writes
//! game state, so it holds **no game logic** (the engine owns the rules; a room's
//! game — once constructed — owns the one game).
//!
//! # Explicit rooms — create with config, join from the directory or by id
//! There is deliberately **no auto-seating** and no matchmaking (ADR 0012). A
//! connection either *creates* a room with a [`RoomConfig`] (a seat count in
//! `2..=8`) — receiving a shareable [`RoomId`] — or *joins* an existing room. Joining
//! a full or unknown room is a typed [`LobbyError`]; the connection's current
//! [`LobbyView`] is re-sent, exactly as an illegal `ChooseAction` re-sends the current
//! `GameView` (`docs/protocol.md`).
//!
//! A room to join no longer has to be discovered out-of-band: every [`LobbyView`]
//! carries a **room directory** ([`LobbyView::directory`], issue #280) — a
//! [`RoomSummary`] per browsable room (id, config, occupancy count, lifecycle state),
//! projected by [`build_directory`] and pushed to every connection on any room
//! lifecycle change ([`broadcast_views`]). It exposes no seat roster, no decklist, and
//! no game state; a started room shows as `in_progress` (visible but not joinable —
//! spectating is out of scope), and a finished room simply leaves the list. This is
//! room *discovery*, still not matchmaking: nothing auto-pairs players.
//!
//! # No game until the pre-game gate passes
//! Creating or joining a room does **not** construct an engine game or send a
//! `GameView`. A room stays in the lobby phase — pushing `LobbyView`s — until every
//! seat is filled, decked (a `submit_deck` whose card identities all resolve against
//! the [`CardDatabase`]), and ready. The instant the last seat readies,
//! [`Lobby::start_game`] builds a [`GameSetup`] from the submitted decks with a
//! server-generated seed, spawns the [`Room`], and pushes each seat a game hand-off;
//! nothing game-related is sent before that moment. This retires the previous
//! "auto-seat into a game that is already live with one player and empty decks"
//! behavior (ADR 0012).
//!
//! # Holding seats for reconnect, and reclaiming rooms
//! A **seated** session is held open across a dropped connection: a disconnect
//! neither vacates the seat nor reclaims the room, so the session's token can later
//! reclaim exactly that seat (issue #113). A **roomless** session holds nothing to
//! reconnect to, so it is dropped outright on disconnect. A pre-game room's registry
//! entry — and the [`Lobby::max_rooms`] capacity it holds — is reclaimed once the room
//! is **empty**, i.e. every seat has been *explicitly* vacated by a `Leave`.
//! Reclamation runs opportunistically on room creation (so freed capacity is available
//! to the next creator, even at the cap) and after every leave. A **started** room
//! ([`RoomEntry::game`] is `Some`) is never reaped: its game task owns the seats'
//! lifecycle now.
//!
//! # Identity and reconnect (issue #113)
//! Every connection is issued an **unguessable** per-session token ([`mint_token`])
//! — a secret, unlike the sequential, public room id — and it is returned in the
//! connection's [`LobbyView`]. A returning connection echoes it on [`Hello`], and
//! [`Lobby::hello`] routes a valid token back into the *same* held seat, resyncing
//! it from one full `LobbyView` (the reconstruct-from-one-view invariant is the
//! resync mechanism). A token is honored only for the seat it was issued for, so it
//! can never reach another player's seat or private state (issue #48). A `Hello`
//! with no token, an unknown one, or one whose room is gone yields a fresh, roomless
//! identity — never someone else's seat. A newer connection presenting a live token
//! **supersedes** the older one (stale-duplicate handling): the older connection's
//! later teardown carries a stale generation and is left inert.
//!
//! # Module layout (issue #409)
//! The lobby state machine is split into focused submodules — pure code motion, with
//! this module as the root retaining the type definitions, the public constructors,
//! and the connection lifecycle (`connect`/`hello`/`disconnect`/`command`) plus
//! [`serve_lobby_connection`]:
//!
//! - [`commands`] — the command handlers (`create_room`, `submit_deck`, `ready`,
//!   `start_game`, and the `join`/`spectate`/`leave`/`set_name` routing).
//! - [`views`] — building and pushing the `LobbyView`/directory/room roster.
//! - [`registry`] — registry and session helpers (seat/room lookup, card and name
//!   validation, seed/token minting).

use std::collections::HashMap;
use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use futures_util::{SinkExt, StreamExt};
use rune_engine::{
    CardDatabase, CardId, CatalogError, FunctionalId, GameSetup, GameState, PlayerSetup,
};
use rune_protocol::{
    CatalogView, CreateRoom, JoinRoom, LobbyCommand, LobbyView, PlayerId, Ready, RoomConfig,
    RoomId, RoomState, RoomSummary, RoomView, SeatView, SessionToken, SetName, SpectateRoom,
    SubmitDeck,
};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::{watch, RwLock};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;
use tracing::{info, warn};

use crate::format::{DeckError, FormatRegistry};
use crate::room::{
    serve_connection, serve_spectator_connection, AutoPassPolicy, Room, RoomHandle, Seat,
};

mod commands;
mod registry;
mod views;

#[cfg(test)]
pub(crate) mod test_support;

pub(crate) use commands::*;
pub(crate) use registry::*;
pub(crate) use views::*;

/// Inclusive range of seats a room may be configured with. The lobby and room
/// plumbing support 2–8 seats even while the engine remains two-player (ADR 0012):
/// a config the engine cannot yet build a game for is caught later, at the ready
/// gate (issue #112), not here.
const SEAT_RANGE: std::ops::RangeInclusive<u8> = 2..=8;

/// The maximum length (in Unicode scalar values) of a player display name (issue
/// #294). Long enough for real names/handles, short enough to keep rosters and
/// in-game labels readable and bound the stored/echoed string. Counts `char`s, not
/// bytes, so a multi-byte name is judged by what a reader sees.
const MAX_NAME_LEN: usize = 32;

/// What the lobby pushes to one connection: either a fresh full [`LobbyView`] to
/// render, or — the instant the ready gate passes — the hand-off that switches the
/// connection to the in-game contract (ADR 0012).
///
/// Not a protocol type: it never touches the wire. The connection task
/// ([`serve_lobby_connection`]) serializes a [`View`](LobbySignal::View) to JSON and
/// writes it back, and on [`Start`](LobbySignal::Start) it reunites its socket and
/// hands off to [`serve_connection`], after which the room speaks `GameView`s.
#[derive(Clone)]
pub(crate) enum LobbySignal {
    /// A fresh pre-game snapshot to serialize and send.
    View(LobbyView),
    /// The gate passed: this connection now owns `seat` of a started room and
    /// should switch to the in-game contract driven by `room`.
    Start {
        /// The seat this connection holds at the table.
        seat: Seat,
        /// Handle to the running room task that now owns the one game.
        room: RoomHandle,
    },
    /// This connection joined as a **spectator** (ADR 0022, issue #351): it should
    /// switch to the read-only spectator bridge driven by `room`, receiving redacted
    /// [`SpectatorView`]s and sending nothing. Like [`Start`](LobbySignal::Start) it is
    /// a terminal hand-off — no `LobbyView` is pushed to a spectating session afterward.
    Spectate {
        /// Handle to the running room task the spectator watches.
        room: RoomHandle,
    },
}

/// Latest-value outbox the lobby pushes a connection's [`LobbySignal`] to. Like the
/// room's per-seat outbox, it is a [`watch`] so a slow reader always observes the
/// newest lobby state and never accumulates a backlog of superseded snapshots.
/// Before the gate it carries [`LobbySignal::View`]s; the single terminal
/// [`LobbySignal::Start`] is never overwritten because no view is pushed to a
/// started seat afterward (see [`push_view`]).
pub(crate) type LobbyOutbox = watch::Sender<Option<LobbySignal>>;

/// The shared session + room registry (layer 1 of `docs/brief.md`).
///
/// Cloning a [`Lobby`] is cheap: every clone shares one registry behind an
/// `Arc<RwLock<...>>`, so each connection task can hold its own handle. The lobby
/// owns the [`CardDatabase`] a room's game is built from and the cap on how many
/// rooms it will host concurrently.
#[derive(Clone)]
pub struct Lobby {
    inner: Arc<Inner>,
}

/// The `Arc`-shared interior of a [`Lobby`].
struct Inner {
    /// The mutable set of sessions and active rooms.
    registry: RwLock<Registry>,
    /// The card database a room's game is built from and decklists are validated
    /// against. The ready gate resolves each submitted [`CardIdentity`] against it
    /// ([`Lobby::submit_deck`]) and constructs the game from the accepted decks
    /// ([`Lobby::start_game`]). The lobby owns the database every room draws from
    /// (ADR 0012).
    ///
    /// [`CardIdentity`]: rune_protocol::CardIdentity
    db: CardDatabase,
    /// The server's format registry (ADR 0013 §4): each room's `game_setup` id is a
    /// key into this, yielding the engine [`GameSetup`] the room starts with plus the
    /// deck-legality rules [`Lobby::submit_deck`] validates a decklist against. A
    /// `CreateRoom` naming an unknown id is rejected before a room opens. Deck
    /// legality is *server* policy, never an engine rule (ADR 0013 §4).
    formats: FormatRegistry,
    /// The cap on concurrently hosted rooms.
    max_rooms: usize,
    /// A fixed engine shuffle seed to build every game from, when set. `None` for
    /// normal play, where [`start_game`](Lobby::start_game) generates a distinct
    /// per-game seed. The seed is server-side state that never reaches a client
    /// (ADR 0014); pinning it makes a whole game reproducible for the end-to-end
    /// suite (issue #145). Sourced from [`Config::rng_seed`](crate::Config::rng_seed).
    seed_override: Option<u64>,
    /// A fixed starting life total to build every game from, when set, overriding
    /// the room format's default (ADR 0013 §4). `None` for normal play. A low value
    /// makes the e2e game reach its lethal `LifeZero` in a few turns (issue #145).
    /// Sourced from [`Config::starting_life`](crate::Config::starting_life).
    life_override: Option<i32>,
}

/// The registry of live sessions and active rooms.
#[derive(Default)]
pub(crate) struct Registry {
    /// The next room id suffix to hand out; only ever increases, so room ids are
    /// never reused.
    next_room: u64,
    /// The next session id suffix to hand out; only ever increases.
    next_session: u64,
    /// Active pre-game rooms, keyed by their opaque [`RoomId`].
    rooms: HashMap<RoomId, RoomEntry>,
    /// Live sessions, keyed by their secret [`SessionToken`].
    sessions: HashMap<SessionToken, Session>,
}

/// A connection's grip on its session: the secret token plus the connection
/// *generation* it was assigned. The generation is bumped every time a new
/// connection attaches to the session (at connect, and on each token reconnect), so
/// a superseded connection can be told apart from the current one: only a handle
/// whose generation still matches the session may tear it down (issue #113
/// stale-duplicate handling).
#[derive(Clone, Debug)]
pub(crate) struct SessionHandle {
    /// The session's secret [`SessionToken`].
    token: SessionToken,
    /// The connection generation this handle was issued for.
    generation: u64,
}

/// One live connection's server-side state.
struct Session {
    /// The public player identity shown to other seats as [`SeatView::occupied_by`].
    player: PlayerId,
    /// The connection's chosen public display name (issue #294), or `None` until it
    /// sets one via [`SetName`]. Bound to the session, so it survives a per-tab
    /// reconnect (the token reclaims this same `Session`). Projected into the lobby
    /// roster ([`SeatView::name`]) and, once a game starts, into every in-game view
    /// ([`GameView::player_names`]). Public information — never redacted beyond the
    /// validation applied when it is set.
    name: Option<String>,
    /// The room this session currently occupies, if any.
    room: Option<RoomId>,
    /// The seat index within [`Session::room`], if seated.
    seat: Option<usize>,
    /// Where this connection's [`LobbySignal`]s are pushed. After a disconnect of a
    /// held (seated) session the receiver is gone, so pushes silently no-op until a
    /// reconnect installs a fresh outbox here.
    outbox: LobbyOutbox,
    /// The generation of the connection currently attached to this session. Bumped
    /// on every (re)attach so a stale, superseded connection's teardown is ignored.
    generation: u64,
}

/// One room: a config, a per-seat occupancy roster, and each seat's pre-game gate
/// state. It holds **no** engine game while pre-game; once the ready gate passes,
/// [`game`](RoomEntry::game) holds the running room task and the seats have switched
/// to the in-game contract (ADR 0012, issue #112).
struct RoomEntry {
    /// The room's configuration, echoed in every [`RoomView`].
    config: RoomConfig,
    /// Per-seat occupancy: the [`SessionToken`] seated at each index, or `None`.
    seats: Vec<Option<SessionToken>>,
    /// Per-seat gate state (submitted deck + ready flag), parallel to
    /// [`seats`](RoomEntry::seats). Kept in a separate vector so seat *occupancy*
    /// stays a plain `Vec<Option<SessionToken>>`.
    gate: Vec<SeatGate>,
    /// The running game once the ready gate has passed; `None` while the room is
    /// still pre-game. A started room is never reaped as "empty" and rejects further
    /// lobby commands — its seats speak `GameView`s now.
    game: Option<RoomHandle>,
    /// The sessions currently **spectating** this room (ADR 0022, issue #351). A
    /// spectator does not occupy a seat, so this is separate from
    /// [`seats`](RoomEntry::seats): the directory advertises `spectators.len()` as the
    /// room's spectator count, independent of seat occupancy. Spectating only starts
    /// once the room's [`game`](RoomEntry::game) is running (there is no pre-game board
    /// to watch), and a spectator is removed on `leave` or disconnect.
    spectators: Vec<SessionToken>,
}

/// One seat's pre-game gate state: the deck it submitted (validated against the
/// card database) and whether it has readied. Decklist *contents* never leave the
/// server — only the derived `decked` flag appears in a [`RoomView`].
#[derive(Clone, Default)]
struct SeatGate {
    /// The seat's validated decklist as engine card ids, or `None` if undecked.
    deck: Option<Vec<CardId>>,
    /// The seat's designated commander (CR 903.3, issue #372) as an engine card id,
    /// or `None` if the seat designated none. Only set alongside a validated
    /// [`deck`](SeatGate::deck); carried here so [`Lobby::start_game`] can hand it to
    /// [`PlayerSetup::with_commander`]. Never leaves the server as deck contents.
    commander: Option<CardId>,
    /// Whether the seat has declared itself ready. A seat may ready only once
    /// [`deck`](SeatGate::deck) is `Some`.
    ready: bool,
}

/// Why a [`LobbyCommand`] was rejected. On any of these the connection's current
/// [`LobbyView`] is re-sent unchanged (ADR 0012); the typed value lets the server
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
    /// `join_room` with an id no active room has.
    UnknownRoom,
    /// `join_room` on a room whose every seat is occupied.
    RoomFull,
    /// `spectate_room` on a room whose game has not started yet (ADR 0022, issue
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
    /// `submit_deck` whose decklist is illegal for the room's format (ADR 0013 §4):
    /// too few or too many cards, or too many copies of a non-basic card. Carries a
    /// [`DeckError`] naming the violation; the seat keeps whatever deck it had.
    IllegalDeck(DeckError),
    /// `ready` (up) on a seat that has not yet submitted a valid deck.
    NotDecked,
    /// A lobby command aimed at a room whose game has already started (its seats
    /// speak `GameView`s now, not lobby commands).
    GameStarted,
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
            Self::UnknownRoom => write!(f, "unknown room id"),
            Self::RoomFull => write!(f, "room is full"),
            Self::RoomNotStarted => write!(f, "room's game has not started yet"),
            Self::AtCapacity => write!(f, "lobby is at room capacity"),
            Self::NotSeated => write!(f, "not seated in a room"),
            Self::UnknownCard(id) => write!(f, "unknown card identity {id}"),
            Self::IllegalDeck(error) => write!(f, "illegal deck: {error}"),
            Self::NotDecked => write!(f, "seat has not submitted a valid deck"),
            Self::GameStarted => write!(f, "the room's game has already started"),
            Self::InvalidName(error) => write!(f, "invalid display name: {error}"),
        }
    }
}

impl std::error::Error for LobbyError {}

impl Lobby {
    /// The default cap on concurrently hosted rooms. Kept modest and explicit for
    /// this milestone; real capacity planning is a later concern (`docs/brief.md`
    /// targets tens of thousands of games per node).
    pub const DEFAULT_MAX_ROOMS: usize = 1024;

    /// Create an empty lobby that builds every room's game from `db` and hosts at
    /// most `max_rooms` rooms at once. Every game is built from a distinct,
    /// server-generated seed and each format's own starting life; use
    /// [`Lobby::with_overrides`] to pin them instead.
    #[must_use]
    pub fn new(db: CardDatabase, max_rooms: usize) -> Self {
        Self::with_overrides(db, max_rooms, None, None)
    }

    /// Create an empty lobby, optionally pinning the engine shuffle `seed_override`
    /// (ADR 0014) and/or a fixed `life_override` starting life every game is built
    /// from (issue #145). Both `None` behaves exactly like [`Lobby::new`]. These are
    /// server/operator concerns (neither reaches a client): a pinned seed makes a
    /// game reproducible, and a low starting life makes it short enough to script
    /// end-to-end. Driven by [`Config`](crate::Config).
    #[must_use]
    pub fn with_overrides(
        db: CardDatabase,
        max_rooms: usize,
        seed_override: Option<u64>,
        life_override: Option<i32>,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                registry: RwLock::new(Registry::default()),
                db,
                formats: FormatRegistry::with_defaults(),
                max_rooms,
                seed_override,
                life_override,
            }),
        }
    }

    /// Create a lobby whose rooms use the engine's bundled card database.
    ///
    /// # Errors
    /// Returns a [`CatalogError`] if the bundled snapshot fails to parse or
    /// validate (see [`CardDatabase::bundled`]).
    pub fn bundled(max_rooms: usize) -> Result<Self, CatalogError> {
        Self::bundled_with_overrides(max_rooms, None, None)
    }

    /// Create a bundled-database lobby, optionally pinning the engine shuffle
    /// `seed_override` and/or a fixed `life_override` (issue #145). See
    /// [`Lobby::with_overrides`].
    ///
    /// # Errors
    /// Returns a [`CatalogError`] if the bundled snapshot fails to parse or
    /// validate (see [`CardDatabase::bundled`]).
    pub fn bundled_with_overrides(
        max_rooms: usize,
        seed_override: Option<u64>,
        life_override: Option<i32>,
    ) -> Result<Self, CatalogError> {
        Ok(Self::with_overrides(
            CardDatabase::bundled()?,
            max_rooms,
            seed_override,
            life_override,
        ))
    }

    /// Build the public card catalog + per-format deck rules (issue #367): every
    /// supported card with its server-generated rules text, and each advertised format's
    /// deck rules and seat range, derived from the one embedded [`CardDatabase`] and the
    /// format registry this lobby owns. Public data only — no deck contents, roster, or
    /// game state, and no session input at all. Answered as a one-shot [`CatalogView`]
    /// frame so a lobby-phase connection can browse the pool without joining a room.
    pub(crate) fn catalog(&self) -> CatalogView {
        crate::catalog::build_catalog(&self.inner.db, &self.inner.formats)
    }

    /// Register a freshly accepted connection: issue it an unguessable session token
    /// and a public identity, store its `outbox`, and push it its initial
    /// [`LobbyView`] (a roomless view offering `create_room`/`join_room`). Returns a
    /// [`SessionHandle`] the connection uses to address, and later tear down, the
    /// session.
    ///
    /// # Errors
    /// Returns the underlying [`getrandom::Error`] if the OS CSPRNG is unavailable:
    /// without unguessable entropy the server cannot safely mint a reconnect token,
    /// so the connection is refused rather than issued a weak one.
    pub(crate) async fn connect(
        &self,
        outbox: LobbyOutbox,
    ) -> Result<SessionHandle, getrandom::Error> {
        let mut registry = self.inner.registry.write().await;
        // The public identity is sequential (it is shown to opponents); the secret
        // token is not — it authenticates reconnect, so it must be unguessable.
        let n = registry.next_session;
        registry.next_session += 1;
        let player = format!("p{n}");
        let token = loop {
            let candidate = mint_token()?;
            if !registry.sessions.contains_key(&candidate) {
                break candidate;
            }
        };
        registry.sessions.insert(
            token.clone(),
            Session {
                player,
                name: None,
                room: None,
                seat: None,
                outbox,
                generation: 0,
            },
        );
        push_view(&registry, &token);
        info!(%token, "connection entered the lobby");
        Ok(SessionHandle {
            token,
            generation: 0,
        })
    }

    /// Route a [`Hello`](rune_protocol::Hello). `current` is the handle the
    /// connection was issued at [`connect`](Lobby::connect) (a fresh identity). If
    /// `echoed` names a *different*, still-known session, this connection proves it
    /// owns that seat by presenting the secret token, so it **supersedes** whatever
    /// connection last held it: the connection's outbox is moved onto the reclaimed
    /// session, the fresh identity is discarded, the session's generation is bumped
    /// (retiring the superseded connection), and the reclaimed session is resynced
    /// from one full [`LobbyView`]. Any other case — no token, the connection's own
    /// token, an unknown token, or one whose room has been reclaimed — keeps the
    /// fresh identity and re-sends its clean, roomless view (the "room gone"
    /// response). Returns the handle the connection should use henceforth.
    pub(crate) async fn hello(
        &self,
        current: &SessionHandle,
        echoed: Option<SessionToken>,
    ) -> SessionHandle {
        let mut registry = self.inner.registry.write().await;

        // Only a *different* token that names a live/held session is a reconnect. An
        // absent, self, unknown, or reaped token falls through to a fresh identity —
        // a token can never resolve to a seat that is not the one it was issued for
        // (issue #48), so a stranger never lands in a held seat.
        let target = echoed
            .as_ref()
            .filter(|t| **t != current.token && registry.sessions.contains_key(*t))
            .cloned();
        let Some(target) = target else {
            push_view(&registry, &current.token);
            return current.clone();
        };

        // Move this connection's outbox onto the reclaimed session and drop the fresh
        // identity `connect` minted, so only the reclaimed session survives.
        let Some(fresh) = registry.sessions.remove(&current.token) else {
            // The current session always exists here; defensively fall back to a
            // no-op reconnect if it somehow does not.
            push_view(&registry, &current.token);
            return current.clone();
        };
        let Some(session) = registry.sessions.get_mut(&target) else {
            // Unreachable: presence was checked above under the same lock. Restore
            // the fresh identity rather than lose the connection's outbox.
            registry.sessions.insert(current.token.clone(), fresh);
            push_view(&registry, &current.token);
            return current.clone();
        };
        session.outbox = fresh.outbox;
        session.generation += 1;
        let generation = session.generation;
        push_view(&registry, &target);
        info!(token = %target, "connection reclaimed a held seat via session token");
        SessionHandle {
            token: target,
            generation,
        }
    }

    /// End a connection. A **seated** session is *held open* — neither its seat nor
    /// its room is touched — so its token can reclaim the seat later (issue #113); a
    /// **roomless** session holds nothing to reconnect to and is removed. The seat is
    /// only ever vacated by an explicit `Leave`.
    ///
    /// The `handle`'s generation must still match the session's, so a **superseded**
    /// connection (an older generation retired by a token reconnect) cannot tear down
    /// the session a newer connection reclaimed. A handle for an already-removed
    /// session is likewise ignored, so a double disconnect cannot corrupt the
    /// registry.
    pub(crate) async fn disconnect(&self, handle: &SessionHandle) {
        let mut registry = self.inner.registry.write().await;
        let Some(session) = registry.sessions.get(&handle.token) else {
            return;
        };
        if session.generation != handle.generation {
            // A newer connection has superseded this one; leave its session intact.
            return;
        }
        // A **spectator** (issue #351) owns no seat, so there is nothing to hold open:
        // drop it from the room's spectator roster (keeping the advertised count
        // accurate) and remove the session. Reconnecting to watch is a fresh
        // `spectate_room`, which reconstructs the whole public board from its first
        // `SpectatorView` — the complete-view principle makes that indistinguishable
        // from resuming.
        if session.room.is_some() && session.seat.is_none() {
            let room_id = session.room.clone();
            registry.sessions.remove(&handle.token);
            if let Some(room_id) = room_id {
                if let Some(room) = registry.rooms.get_mut(&room_id) {
                    room.spectators.retain(|t| *t != handle.token);
                }
            }
            broadcast_views(&registry);
            info!(token = %handle.token, "spectator connection left");
            return;
        }
        if session.room.is_some() {
            info!(token = %handle.token, "connection dropped; seat held open for reconnect");
            return;
        }
        registry.sessions.remove(&handle.token);
        info!(token = %handle.token, "connection left the lobby");
    }

    /// Route one [`LobbyCommand`] from `token` against authoritative state. On
    /// success the affected connections are pushed a fresh [`LobbyView`]; on a typed
    /// [`LobbyError`] the sender's current view is re-sent unchanged and the error is
    /// returned (for logging/tests).
    pub(crate) async fn command(
        &self,
        token: &SessionToken,
        command: LobbyCommand,
    ) -> Result<(), LobbyError> {
        let mut registry = self.inner.registry.write().await;
        if !registry.sessions.contains_key(token) {
            return Err(LobbyError::UnknownSession);
        }
        let result = match command {
            // Reconnect-by-token is driven by [`Lobby::hello`] from the serve loop,
            // which can supersede the connection's identity (a generation change this
            // token-only router cannot express). A `Hello` reaching here — e.g. a
            // direct call in a test — is a harmless ack that re-sends the current view.
            LobbyCommand::Hello(_) => Ok(()),
            LobbyCommand::CreateRoom(CreateRoom { config }) => {
                self.create_room(&mut registry, token, config)
            }
            LobbyCommand::JoinRoom(JoinRoom { room_id }) => {
                join_room(&mut registry, token, &room_id)
            }
            LobbyCommand::SpectateRoom(SpectateRoom { room_id }) => {
                spectate_room(&mut registry, token, &room_id)
            }
            LobbyCommand::Leave => leave_room(&mut registry, token),
            LobbyCommand::SubmitDeck(SubmitDeck { cards, commander }) => {
                self.submit_deck(&mut registry, token, &cards, commander.as_deref())
            }
            LobbyCommand::Ready(Ready { ready }) => self.ready(&mut registry, token, ready),
            LobbyCommand::SetName(SetName { name }) => set_name(&mut registry, token, &name),
            // A catalog request is answered directly by the serve loop with a one-shot
            // `CatalogView` (it needs the socket, not this registry), so a request
            // reaching this router — e.g. a direct call in a test — is a harmless ack
            // that re-sends the current view (issue #367).
            LobbyCommand::RequestCatalog => Ok(()),
        };
        // Whether the command succeeded (and already pushed the affected views) or
        // was rejected, the sender always ends with a fresh, authoritative view.
        push_view(&registry, token);
        result
    }
}

/// Bridge a live WebSocket connection to the lobby for its pre-game phase.
///
/// This is the pre-game analogue of [`serve_connection`](crate::serve_connection):
/// it registers a session (receiving the initial [`LobbyView`]), then pumps the
/// socket both ways until either side closes. Decoded [`LobbyCommand`]s are routed
/// through [`Lobby::command`]; every [`LobbyView`] the lobby pushes is serialized to
/// JSON and written back. On exit the session is disconnected — a **seated** session
/// has its seat held open for reconnect (issue #113), a roomless one is dropped.
///
/// It carries **no game logic** — it only (de)serializes the lobby protocol and
/// routes commands to the authoritative registry. Constructing the engine game is
/// the lobby's job at the ready gate; when it fires, this bridge learns of it via a
/// [`LobbySignal::Start`] on the outbox, reunites its socket, and **hands off to
/// [`serve_connection`]** — from there the connection speaks the in-game `GameView`
/// contract for the life of that game. Nothing game-related is written before that.
///
/// `shutdown` lets the layer-1 server stop the bridge on server shutdown: when it
/// resolves, the session is released and the socket is closed politely, just as if
/// the peer had hung up. It is forwarded to the in-game bridge on hand-off, so a
/// started game shuts down cleanly too.
pub async fn serve_lobby_connection<S, F>(lobby: Lobby, ws: WebSocketStream<S>, shutdown: F)
where
    S: AsyncRead + AsyncWrite + Unpin,
    F: Future<Output = ()>,
{
    let (mut write, mut read) = ws.split();
    let (outbox_tx, mut outbox_rx) = watch::channel::<Option<LobbySignal>>(None);
    // Registering the session pushes the initial LobbyView onto the outbox, so the
    // writer arm below sends it as the connection's first frame. The handle can be
    // reassigned mid-connection when a `Hello` reconnects to a held seat.
    let mut handle = match lobby.connect(outbox_tx).await {
        Ok(handle) => handle,
        Err(error) => {
            // Without OS entropy we cannot mint an unguessable token; refuse rather
            // than issue a weak one.
            warn!(%error, "failed to mint a session token; closing connection");
            let _ = write.close().await;
            return;
        }
    };

    // Set once the ready gate hands this connection off to a started game.
    let mut handoff: Option<(Seat, RoomHandle)> = None;
    // Set once this connection joins a running game as a spectator (issue #351).
    let mut spectate_handoff: Option<RoomHandle> = None;

    tokio::pin!(shutdown);
    loop {
        tokio::select! {
            () = &mut shutdown => break,
            incoming = read.next() => match incoming {
                Some(Ok(Message::Text(text))) => {
                    match serde_json::from_str::<LobbyCommand>(text.as_str()) {
                        // The catalog is static reference data, not per-connection lobby
                        // state (issue #367): answer it directly with a one-shot
                        // `CatalogView` frame rather than through the latest-value outbox,
                        // which only carries `LobbyView`s and could drop the response
                        // under a concurrent directory broadcast.
                        Ok(LobbyCommand::RequestCatalog) => {
                            match serde_json::to_string(&lobby.catalog()) {
                                Ok(json) => {
                                    if write.send(Message::Text(json)).await.is_err() {
                                        break;
                                    }
                                }
                                Err(error) => {
                                    warn!(token = %handle.token, %error, "failed to serialize catalog view");
                                }
                            }
                        }
                        // A `Hello` may reconnect this connection to a held seat and hand
                        // back a new identity, so `handle` is updated in place.
                        Ok(LobbyCommand::Hello(hello)) => {
                            handle = lobby.hello(&handle, hello.token).await;
                        }
                        // Every other command routes through the authoritative registry.
                        Ok(command) => {
                            if let Err(error) = lobby.command(&handle.token, command).await {
                                warn!(token = %handle.token, %error, "rejected lobby command");
                            }
                        }
                        Err(error) => {
                            warn!(token = %handle.token, %error, "ignoring undecodable lobby command");
                        }
                    }
                }
                Some(Ok(Message::Ping(payload))) => {
                    if write.send(Message::Pong(payload)).await.is_err() {
                        break;
                    }
                }
                Some(Ok(Message::Close(_))) | None => break,
                Some(Ok(_)) => {} // binary/pong/raw frames carry no protocol message
                Some(Err(error)) => {
                    warn!(token = %handle.token, %error, "websocket read error");
                    break;
                }
            },
            // Latest-value outbox: while parked on a slow `write.send`, the lobby may
            // overwrite the pending view any number of times; we serialize only the
            // newest snapshot when we loop back. Safe because each `LobbyView` is a
            // complete snapshot (`docs/protocol.md`), so superseded ones can be
            // dropped; the channel never grows under a slow reader.
            changed = outbox_rx.changed() => match changed {
                Ok(()) => {
                    let latest = outbox_rx.borrow_and_update().clone();
                    match latest {
                        Some(LobbySignal::View(view)) => match serde_json::to_string(&view) {
                            Ok(json) => {
                                if write.send(Message::Text(json)).await.is_err() {
                                    break;
                                }
                            }
                            Err(error) => {
                                warn!(token = %handle.token, %error, "failed to serialize lobby view");
                            }
                        },
                        // The ready gate passed: stop serving the lobby and hand off
                        // to the in-game contract below.
                        Some(LobbySignal::Start { seat, room }) => {
                            handoff = Some((seat, room));
                            break;
                        }
                        // Joined a running game as a spectator: hand off to the
                        // read-only spectator bridge below (issue #351).
                        Some(LobbySignal::Spectate { room }) => {
                            spectate_handoff = Some(room);
                            break;
                        }
                        None => {}
                    }
                }
                Err(_) => break,
            },
        }
    }

    if let Some((seat, room)) = handoff {
        // Reunite the split socket and switch to the in-game bridge. The session is
        // *not* disconnected: its seat is now the game's, held open for reconnect
        // (issue #113). The shutdown future carries over so the game bridge still
        // closes cleanly on server shutdown.
        match write.reunite(read) {
            Ok(ws) => serve_connection(seat, room, ws, shutdown).await,
            Err(error) => {
                warn!(token = %handle.token, %error, "failed to reunite socket for game hand-off")
            }
        }
        return;
    }

    if let Some(room) = spectate_handoff {
        // Reunite the socket and switch to the read-only spectator bridge (issue #351).
        // On exit the spectator is dropped from the lobby (it holds no seat to keep).
        match write.reunite(read) {
            Ok(ws) => serve_spectator_connection(room, ws, shutdown).await,
            Err(error) => {
                warn!(token = %handle.token, %error, "failed to reunite socket for spectator hand-off")
            }
        }
        lobby.disconnect(&handle).await;
        return;
    }

    lobby.disconnect(&handle).await;
    let _ = write.close().await;
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use crate::lobby::test_support::*;

    use rune_protocol::Hello;

    #[tokio::test]
    async fn issue_367_a_lobby_connection_obtains_the_catalog_without_a_room_or_game() {
        // A fresh, roomless connection can browse the full catalog: every supported card
        // and every advertised format, with no room joined and no game constructed.
        let lobby = lobby(4);
        let alice = Client::connect(&lobby).await;
        assert!(alice.current().room.is_none(), "no room joined");
        assert!(!alice.started(), "no game constructed");

        let catalog = lobby.catalog();
        assert!(!catalog.cards.is_empty(), "the catalog lists cards");
        assert!(!catalog.formats.is_empty(), "the catalog lists formats");
        // It projects the whole bundled database.
        assert_eq!(catalog.cards.len(), CardDatabase::bundled().unwrap().len());

        // Routing the request through the registry is a harmless ack that changes no
        // lobby state — the connection stays roomless and no game starts (issue #367).
        lobby
            .command(&alice.token, LobbyCommand::RequestCatalog)
            .await
            .expect("request_catalog is accepted");
        assert!(alice.current().room.is_none());
        assert!(!alice.started());
    }

    #[tokio::test]
    async fn a_new_connection_lands_in_the_lobby_with_a_session_and_no_game() {
        let lobby = lobby(4);
        let mut client = Client::connect(&lobby).await;
        let view = client.view().await;

        // Issued a session token and a public identity; not in any room.
        assert!(!view.session.is_empty());
        assert!(!view.you.is_empty());
        assert!(view.room.is_none());
        // Only the create/join/spectate commands are legal before a room exists.
        assert_eq!(
            view.valid_commands,
            vec![
                "set_name".to_string(),
                "create_room".to_string(),
                "join_room".to_string(),
                "spectate_room".to_string()
            ]
        );
    }

    #[tokio::test]
    async fn a_dropped_connection_holds_its_seat_open_for_token_reconnect() {
        // Reconnect model (issue #113): a disconnect no longer vacates a seat or
        // reclaims the room — the seat is held so the token can return to it. Only an
        // explicit `Leave` vacates (covered by the reclamation tests below).
        let lobby = lobby(4);
        let mut alice = Client::connect(&lobby).await;
        let _ = alice.view().await;
        lobby
            .command(
                &alice.token,
                LobbyCommand::CreateRoom(CreateRoom { config: config(2) }),
            )
            .await
            .unwrap();
        let room_id = alice.view().await.room.unwrap().room_id;
        let alice_you = alice.current().you.clone();
        let alice_token = alice.token.clone();

        // Alice's socket drops. Her seat is HELD open and the room is NOT reclaimed.
        lobby.disconnect(&alice.handle()).await;

        // A brand-new joiner takes the *other* seat — never Alice's held seat 0 — and
        // the room is proven to still exist (it was not reclaimed on her disconnect).
        let mut bob = Client::connect(&lobby).await;
        let _ = bob.view().await;
        lobby
            .command(
                &bob.token,
                LobbyCommand::JoinRoom(JoinRoom {
                    room_id: room_id.clone(),
                }),
            )
            .await
            .expect("the held-open room still accepts a joiner");
        let bob_room = bob
            .view()
            .await
            .room
            .expect("bob joined the surviving room");
        assert_eq!(bob_room.room_id, room_id);
        assert_eq!(
            bob_room.seats[1].occupied_by.as_deref(),
            Some(bob.current().you.as_str())
        );
        assert_eq!(
            bob_room.seats[0].occupied_by.as_deref(),
            Some(alice_you.as_str()),
            "Alice's seat is still held while she is away",
        );

        // Alice reconnects with her token and is resynced into the *same* seat 0.
        let alice2 = Client::reconnect(&lobby, Some(alice_token)).await;
        let resumed = alice2.current().room.expect("alice reclaims her held room");
        assert_eq!(resumed.room_id, room_id);
        assert_eq!(
            resumed.seats[0].occupied_by.as_deref(),
            Some(alice_you.as_str())
        );
        assert_eq!(
            alice2.current().you,
            alice_you,
            "same identity across reconnect"
        );
    }

    /// Regression for issue #113, referencing issue #48 (the hidden-hand leak the
    /// one-way seat retirement was guarding against). A held seat is handed back
    /// **only** to the exact secret token that owns it: a returning stranger — no
    /// token, a forged token, or another seat's *public* identity — never lands in
    /// someone else's held seat. That is precisely what stops a reconnect from
    /// leaking the private state a held seat guards (in game, the absent player's
    /// hand and library; #48), and the token never resolves to a *different* seat.
    #[tokio::test]
    async fn issue_113_reconnect_token_never_leaks_a_held_seat_referencing_48() {
        let lobby = lobby(4);

        // Alice opens a room (seat 0); Bob joins (seat 1).
        let mut alice = Client::connect(&lobby).await;
        let _ = alice.view().await;
        lobby
            .command(
                &alice.token,
                LobbyCommand::CreateRoom(CreateRoom { config: config(2) }),
            )
            .await
            .unwrap();
        let room_id = alice.view().await.room.unwrap().room_id;
        let alice_you = alice.current().you.clone();
        let alice_token = alice.token.clone();

        let mut bob = Client::connect(&lobby).await;
        let _ = bob.view().await;
        lobby
            .command(
                &bob.token,
                LobbyCommand::JoinRoom(JoinRoom {
                    room_id: room_id.clone(),
                }),
            )
            .await
            .unwrap();
        let _ = bob.view().await;
        let _ = alice.view().await;

        // Alice's socket drops; seat 0 is held open, still showing her as occupant.
        lobby.disconnect(&alice.handle()).await;

        // A stranger tries every value they might present: no token, a forged token,
        // and Alice's PUBLIC identity (which other seats legitimately see). None is
        // her secret session token, so none reclaims her seat — each yields a fresh,
        // roomless identity that never sees the inside of the room.
        for forged in [
            None,
            Some("s-forged-guess".to_string()),
            Some(alice_you.clone()),
        ] {
            let stranger = Client::reconnect(&lobby, forged).await;
            let view = stranger.current();
            assert!(view.room.is_none(), "a stranger never lands in a held seat");
            assert_ne!(
                view.session, alice_token,
                "a stranger is never handed Alice's secret token",
            );
            assert_ne!(
                view.you, alice_you,
                "a stranger gets its own identity, never Alice's seat",
            );
        }

        // Only the real secret token reclaims the seat — and always the SAME seat 0,
        // never Bob's seat 1.
        let alice2 = Client::reconnect(&lobby, Some(alice_token)).await;
        let resumed = alice2
            .current()
            .room
            .expect("the true token reclaims the seat");
        assert_eq!(resumed.room_id, room_id);
        assert_eq!(
            resumed.seats[0].occupied_by.as_deref(),
            Some(alice_you.as_str()),
            "reclaimed her own seat 0",
        );
        assert_ne!(
            resumed.seats[1].occupied_by.as_deref(),
            Some(alice_you.as_str()),
            "the token never grants a different seat",
        );
    }

    #[tokio::test]
    async fn issue_113_a_new_connection_with_the_token_supersedes_the_old_one() {
        // Stale-duplicate handling: the new connection supersedes the old. Even while
        // the old connection is still nominally alive, presenting its token takes the
        // seat over, and the old connection's later teardown is a no-op (stale
        // generation) — so it cannot vacate the seat the new connection now holds.
        let lobby = lobby(4);
        let mut alice = Client::connect(&lobby).await;
        let _ = alice.view().await;
        lobby
            .command(
                &alice.token,
                LobbyCommand::CreateRoom(CreateRoom { config: config(2) }),
            )
            .await
            .unwrap();
        let _ = alice.view().await;
        let alice_token = alice.token.clone();
        let stale_handle = alice.handle();

        // A new connection presents the same token and reclaims the seat.
        let alice2 = Client::reconnect(&lobby, Some(alice_token.clone())).await;
        assert!(
            alice2.current().room.is_some(),
            "the superseding connection holds the seat",
        );
        assert_eq!(alice2.token, alice_token, "same session, new connection");

        // The OLD connection tears down: its generation is stale, so this is ignored
        // and the reclaimed session survives.
        lobby.disconnect(&stale_handle).await;

        // Proof the seat is still held for the new connection: it can reconnect again.
        let alice3 = Client::reconnect(&lobby, Some(alice_token)).await;
        assert!(
            alice3.current().room.is_some(),
            "the superseding connection still owns the held seat",
        );
    }

    #[tokio::test]
    async fn hello_command_is_acknowledged_with_a_fresh_view() {
        // A `Hello` routed through `command` (rather than the serve loop's reconnect
        // path) is a harmless ack that re-sends the current view.
        let lobby = lobby(4);
        let mut alice = Client::connect(&lobby).await;
        let first = alice.view().await;
        lobby
            .command(&alice.token, LobbyCommand::Hello(Hello::default()))
            .await
            .expect("hello acknowledged");
        let again = alice.view().await;
        assert_eq!(again.session, first.session);
        assert!(again.room.is_none());
    }

    #[tokio::test]
    async fn hello_without_a_token_keeps_the_fresh_identity() {
        // A first-contact Hello (no token) has nothing to reclaim: the connection
        // keeps the identity it was minted at connect and is re-sent its view.
        let lobby = lobby(4);
        let mut alice = Client::connect(&lobby).await;
        let first = alice.view().await;
        let handle = alice.handle();
        let after = lobby.hello(&handle, None).await;
        assert_eq!(after.token, first.session, "identity unchanged");
        assert_eq!(after.generation, handle.generation, "no supersede");
        let again = alice.view().await;
        assert_eq!(again.session, first.session);
        assert!(again.room.is_none());
    }

    #[tokio::test]
    async fn hello_with_an_unknown_token_gets_a_clean_roomless_view() {
        // A token for a session/room that no longer exists (the "room gone" case)
        // never resolves to another seat: the connection keeps its fresh, roomless
        // identity rather than being routed anywhere.
        let lobby = lobby(4);
        let mut alice = Client::connect(&lobby).await;
        let first = alice.view().await;
        let handle = alice.handle();
        let after = lobby
            .hello(&handle, Some("s-does-not-exist".to_string()))
            .await;
        assert_eq!(
            after.token, first.session,
            "unknown token grants no other seat"
        );
        let again = alice.view().await;
        assert!(again.room.is_none(), "clean roomless lobby response");
    }

    #[tokio::test]
    async fn a_command_from_an_unknown_session_is_rejected() {
        let lobby = lobby(4);
        assert_eq!(
            lobby
                .command(&"s-nope".to_string(), LobbyCommand::Leave)
                .await,
            Err(LobbyError::UnknownSession)
        );
    }
}
