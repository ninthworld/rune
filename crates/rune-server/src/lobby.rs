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
    CreateRoom, JoinRoom, LobbyCommand, LobbyView, PlayerId, Ready, RoomConfig, RoomId, RoomState,
    RoomSummary, RoomView, SeatView, SessionToken, SetName, SpectateRoom, SubmitDeck,
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
struct Registry {
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
            LobbyCommand::SubmitDeck(SubmitDeck { cards }) => {
                self.submit_deck(&mut registry, token, &cards)
            }
            LobbyCommand::Ready(Ready { ready }) => self.ready(&mut registry, token, ready),
            LobbyCommand::SetName(SetName { name }) => set_name(&mut registry, token, &name),
        };
        // Whether the command succeeded (and already pushed the affected views) or
        // was rejected, the sender always ends with a fresh, authoritative view.
        push_view(&registry, token);
        result
    }

    /// Handle `create_room`: validate the config, reap empty rooms, enforce the room
    /// cap, then open a room and seat the creator at seat 0.
    fn create_room(
        &self,
        registry: &mut Registry,
        token: &SessionToken,
        config: RoomConfig,
    ) -> Result<(), LobbyError> {
        if registry
            .sessions
            .get(token)
            .is_some_and(|s| s.room.is_some())
        {
            return Err(LobbyError::AlreadyInRoom);
        }
        if !SEAT_RANGE.contains(&config.seats) {
            return Err(LobbyError::InvalidSeatCount(config.seats));
        }
        // The `game_setup` id must name a registered format (ADR 0013 §4); an unknown
        // id is refused before a room is opened, so no room ever holds a setup the
        // server cannot build a game from or validate decks against.
        let Some(format) = self.inner.formats.get(&config.game_setup) else {
            return Err(LobbyError::UnknownFormat(config.game_setup.clone()));
        };
        // The seat count must also be one the chosen format allows (issue #349): a
        // two-player format refuses a free-for-all count, and a free-for-all refuses a
        // duel. Non-fatal — the current lobby view is re-sent, like every other
        // rejected command.
        if !format.seats.contains(&config.seats) {
            return Err(LobbyError::SeatCountForFormat {
                seats: config.seats,
                format: config.game_setup.clone(),
            });
        }
        // Free capacity held by empty rooms before checking the cap, so a creator is
        // never refused for a slot no live room still needs.
        reap_empty(registry);
        if registry.rooms.len() >= self.inner.max_rooms {
            return Err(LobbyError::AtCapacity);
        }

        let n = registry.next_room;
        registry.next_room += 1;
        let room_id = format!("r{n}");
        let seat_count = config.seats as usize;
        let mut seats = vec![None; seat_count];
        seats[0] = Some(token.clone());
        registry.rooms.insert(
            room_id.clone(),
            RoomEntry {
                config,
                seats,
                gate: vec![SeatGate::default(); seat_count],
                game: None,
                spectators: Vec::new(),
            },
        );
        if let Some(session) = registry.sessions.get_mut(token) {
            session.room = Some(room_id.clone());
            session.seat = Some(0);
        }
        // A new room appeared in the directory: re-project it to everyone browsing.
        broadcast_views(registry);
        info!(%token, %room_id, "opened room");
        Ok(())
    }

    /// Handle `submit_deck`: resolve every card identity against the database, then
    /// validate the whole decklist against the room's **format** (ADR 0013 §4) and,
    /// on success, store the seat's deck (leaving it decked) and re-notify the room.
    ///
    /// Validation is authoritative and all-or-nothing, in two stages: first the first
    /// identity that does not resolve rejects the whole command with
    /// [`LobbyError::UnknownCard`] ("unknown ids → typed error, seat stays undecked",
    /// ADR 0012); then the resolved deck is checked against the format's deck-legality
    /// rules — size and per-card copy limit — and an illegal deck is rejected with a
    /// structured [`LobbyError::IllegalDeck`] naming the violation (ADR 0013 §4). On
    /// any rejection the seat keeps whatever deck it had (it stays undecked if it had
    /// none). Re-submitting a legal deck clears that seat's ready flag, so a changed
    /// deck must be re-readied. Deck legality is *server* policy, never an engine rule.
    fn submit_deck(
        &self,
        registry: &mut Registry,
        token: &SessionToken,
        cards: &[String],
    ) -> Result<(), LobbyError> {
        let (room_id, seat) = seat_of(registry, token)?;
        let room = registry
            .rooms
            .get_mut(&room_id)
            .ok_or(LobbyError::NotSeated)?;
        if room.game.is_some() {
            return Err(LobbyError::GameStarted);
        }
        // Resolve the whole list before mutating, so a bad identity leaves the seat's
        // existing gate state untouched.
        let mut deck = Vec::with_capacity(cards.len());
        for identity in cards {
            let card = resolve_card(&self.inner.db, identity)
                .ok_or_else(|| LobbyError::UnknownCard(identity.clone()))?;
            deck.push(card);
        }
        // Validate the resolved deck against the room's format before storing it, so
        // an illegal deck never seats a broken game (ADR 0013 §4). The format is
        // guaranteed present: `create_room` rejected any unknown `game_setup` id.
        if let Some(format) = self.inner.formats.get(&room.config.game_setup) {
            format
                .validate_deck(&deck, &self.inner.db)
                .map_err(LobbyError::IllegalDeck)?;
        }
        if let Some(gate) = room.gate.get_mut(seat) {
            gate.deck = Some(deck);
            gate.ready = false;
        }
        push_room(registry, &room_id);
        info!(%token, %room_id, seat, "seat submitted a valid deck");
        Ok(())
    }

    /// Handle `ready`: toggle the seat's ready flag, then — when readying up completes
    /// the gate — construct the game and hand every seat off to the in-game contract.
    ///
    /// Readying up requires a submitted deck ([`LobbyError::NotDecked`] otherwise);
    /// un-readying (`ready == false`) is always allowed for a seated player before the
    /// game starts. When the last seat readies and every seat is filled, decked, and
    /// ready, [`start_game`](Lobby::start_game) builds the `GameState` and switches the
    /// room to the game phase (ADR 0012).
    fn ready(
        &self,
        registry: &mut Registry,
        token: &SessionToken,
        ready: bool,
    ) -> Result<(), LobbyError> {
        let (room_id, seat) = seat_of(registry, token)?;
        let room = registry
            .rooms
            .get_mut(&room_id)
            .ok_or(LobbyError::NotSeated)?;
        if room.game.is_some() {
            return Err(LobbyError::GameStarted);
        }
        if ready && room.gate.get(seat).is_none_or(|g| g.deck.is_none()) {
            return Err(LobbyError::NotDecked);
        }
        if let Some(gate) = room.gate.get_mut(seat) {
            gate.ready = ready;
        }
        // Everyone in the room sees the changed ready flag.
        push_room(registry, &room_id);
        if ready {
            self.start_game(registry, &room_id);
        }
        info!(%token, %room_id, seat, ready, "seat toggled ready");
        Ok(())
    }

    /// Construct the game and hand off, but only if the room is fully gated: every
    /// seat occupied, decked, and ready. Otherwise a no-op — the room stays pre-game.
    ///
    /// On the gate passing, builds the room format's engine [`GameSetup`] (ADR 0013
    /// §4) from the seats' submitted decks in seat order with a server-generated seed,
    /// spawns a [`Room`] around
    /// [`GameState::new`], stores its handle on the [`RoomEntry`], and pushes each
    /// seated session a [`LobbySignal::Start`] carrying its seat and the room handle.
    /// Each connection then reunites its socket and switches to `serve_connection`
    /// (`GameView`s from here on). If construction fails — which cannot happen once
    /// every deck validated at submit against the same database — the game is not
    /// started and the room stays pre-game (logged), rather than panicking.
    fn start_game(&self, registry: &mut Registry, room_id: &RoomId) {
        let Some(room) = registry.rooms.get(room_id) else {
            return;
        };
        // Gate: every seat filled, decked, and ready.
        let ready_to_start = room
            .seats
            .iter()
            .zip(&room.gate)
            .all(|(occupant, gate)| occupant.is_some() && gate.deck.is_some() && gate.ready);
        if !ready_to_start {
            return;
        }

        // Build the setup from each seat's deck, in seat order.
        let players: Vec<PlayerSetup> = room
            .gate
            .iter()
            .map(|gate| PlayerSetup::new(gate.deck.clone().unwrap_or_default()))
            .collect();
        // Seed the shuffle: a pinned override (deterministic games for the e2e
        // suite, ADR 0014 / issue #145) if configured, else a fresh per-game seed.
        let seed = self.inner.seed_override.unwrap_or_else(generate_seed);
        // The format supplies the engine `GameSetup` parameters (ADR 0013 §4); it is
        // guaranteed present (create_room rejected any unknown id), but fall back to
        // engine defaults rather than panicking if it is somehow absent.
        let mut setup: GameSetup = match self.inner.formats.get(&room.config.game_setup) {
            Some(format) => format.game_setup(players, seed),
            None => GameSetup::new(players, seed),
        };
        // A pinned starting life (e2e short game, issue #145) overrides the format's
        // default; normal play keeps the format's value.
        if let Some(life) = self.inner.life_override {
            setup.starting_life = life;
        }
        // Each seat's chosen display name in seat order (issue #294), so the room can
        // label players in every `GameView::player_names`. A seat with no name is `None`
        // and simply has no entry in the projected map.
        let player_names: Vec<Option<String>> = room
            .seats
            .iter()
            .map(|occupant| {
                occupant
                    .as_ref()
                    .and_then(|token| registry.sessions.get(token))
                    .and_then(|session| session.name.clone())
            })
            .collect();
        let db = self.inner.db.clone();
        let state = match GameState::new(&setup, &db) {
            Ok(state) => state,
            Err(error) => {
                // Unreachable in practice: every card id was validated at submit.
                warn!(%room_id, %error, "game construction failed; room stays pre-game");
                return;
            }
        };
        // Basic priority automation is on for real games (issue #264): an idle seat's
        // priority auto-passes so a spell-less turn does not cost a click per step,
        // gated by each seat's own `set_stops` preferences. Off only in unit tests
        // that drive priority pass-by-pass.
        let (handle, _task) = Room::new(state, db)
            .with_player_names(player_names)
            .with_auto_pass(AutoPassPolicy::On)
            .spawn();

        // Hand every seated session off to the in-game contract.
        let occupants: Vec<(Seat, SessionToken)> = room
            .seats
            .iter()
            .enumerate()
            .filter_map(|(seat, occupant)| occupant.clone().map(|token| (seat, token)))
            .collect();
        for (seat, token) in &occupants {
            if let Some(session) = registry.sessions.get(token) {
                let _ = session.outbox.send(Some(LobbySignal::Start {
                    seat: *seat,
                    room: handle.clone(),
                }));
            }
        }
        // Mark the room started so it rejects further lobby commands and is never
        // reaped as empty. The task handle keeps the room alive alongside the
        // connections' own handles.
        if let Some(room) = registry.rooms.get_mut(room_id) {
            room.game = Some(handle);
        }
        // The room flipped to `in_progress` in the directory: re-project to everyone
        // browsing (the room's own seats are on the in-game contract now and are
        // skipped by `push_view`, so their terminal `Start` hand-off is preserved).
        broadcast_views(registry);
        info!(%room_id, seats = occupants.len(), "ready gate passed; game constructed");
    }
}

/// Handle `join_room`: seat the joiner in the first free seat of an existing room,
/// or return a typed error for an unknown or full room.
fn join_room(
    registry: &mut Registry,
    token: &SessionToken,
    room_id: &RoomId,
) -> Result<(), LobbyError> {
    if registry
        .sessions
        .get(token)
        .is_some_and(|s| s.room.is_some())
    {
        return Err(LobbyError::AlreadyInRoom);
    }
    let room = registry
        .rooms
        .get_mut(room_id)
        .ok_or(LobbyError::UnknownRoom)?;
    let seat = room
        .seats
        .iter()
        .position(Option::is_none)
        .ok_or(LobbyError::RoomFull)?;
    room.seats[seat] = Some(token.clone());
    if let Some(session) = registry.sessions.get_mut(token) {
        session.room = Some(room_id.clone());
        session.seat = Some(seat);
    }
    // Every occupant's roster changed, and the room's occupancy changed in the
    // directory: re-project to occupants and to everyone browsing.
    broadcast_views(registry);
    info!(%token, %room_id, seat, "joined room");
    Ok(())
}

/// Handle `spectate_room` (ADR 0022, issue #351): attach the sender as a spectator of
/// an **in-progress** room without consuming a seat. Unlike [`join_room`] this succeeds
/// on a room whose seats are full, but the room's game must already be running — there
/// is no board to watch until the ready gate passes ([`LobbyError::RoomNotStarted`]).
/// On success the session is marked as spectating (`room` set, `seat` left `None`), the
/// room's spectator roster gains its token (advertised as a count in the directory),
/// and the connection is handed off to the read-only spectator bridge via
/// [`LobbySignal::Spectate`].
fn spectate_room(
    registry: &mut Registry,
    token: &SessionToken,
    room_id: &RoomId,
) -> Result<(), LobbyError> {
    if registry
        .sessions
        .get(token)
        .is_some_and(|s| s.room.is_some())
    {
        return Err(LobbyError::AlreadyInRoom);
    }
    let room = registry
        .rooms
        .get_mut(room_id)
        .ok_or(LobbyError::UnknownRoom)?;
    // A spectator needs a live game to watch. A gathering room has no board yet.
    let handle = match &room.game {
        Some(handle) if handle.is_active() => handle.clone(),
        _ => return Err(LobbyError::RoomNotStarted),
    };
    room.spectators.push(token.clone());
    if let Some(session) = registry.sessions.get(token) {
        // Hand this connection off to the read-only spectator contract immediately —
        // like the `Start` gate, a terminal signal after which no `LobbyView` is pushed.
        let _ = session
            .outbox
            .send(Some(LobbySignal::Spectate { room: handle }));
    }
    if let Some(session) = registry.sessions.get_mut(token) {
        session.room = Some(room_id.clone());
        session.seat = None;
    }
    // The room's spectator count changed in the directory: re-project to browsers.
    broadcast_views(registry);
    info!(%token, %room_id, "joined room as spectator");
    Ok(())
}

/// Handle `leave`: vacate the sender's seat, reclaim the room if it is now empty,
/// otherwise notify the remaining occupants.
fn leave_room(registry: &mut Registry, token: &SessionToken) -> Result<(), LobbyError> {
    let (room_id, seat) = match registry.sessions.get(token) {
        Some(Session {
            room: Some(room_id),
            seat,
            ..
        }) => (room_id.clone(), *seat),
        _ => return Err(LobbyError::NotInRoom),
    };
    // A spectator (issue #351) holds no seat: drop it from the room's spectator roster
    // instead of vacating a seat, then clear its session and re-project the directory
    // (its spectator count changed). The room is never reaped for losing a spectator.
    let Some(seat) = seat else {
        if let Some(room) = registry.rooms.get_mut(&room_id) {
            room.spectators.retain(|t| t != token);
        }
        if let Some(session) = registry.sessions.get_mut(token) {
            session.room = None;
        }
        broadcast_views(registry);
        info!(%token, %room_id, "stopped spectating room");
        return Ok(());
    };
    vacate(registry, &room_id, seat);
    if let Some(session) = registry.sessions.get_mut(token) {
        session.room = None;
        session.seat = None;
    }
    reap_empty(registry);
    // The room's occupancy changed (or it was reclaimed and left the directory):
    // re-project to its remaining occupants and to everyone browsing.
    broadcast_views(registry);
    info!(%token, %room_id, seat, "left room");
    Ok(())
}

/// Handle `set_name`: validate the requested display name and store it on the
/// session (issue #294). On success the affected views are re-pushed — the sender's
/// own, and, if it is seated, the whole room roster so every occupant sees the new
/// name. On rejection the name is left untouched and a typed [`LobbyError::InvalidName`]
/// is returned; the caller re-sends the sender's current [`LobbyView`] unchanged (the
/// lobby's non-fatal error pattern).
fn set_name(
    registry: &mut Registry,
    token: &SessionToken,
    requested: &str,
) -> Result<(), LobbyError> {
    let name = validate_name(requested).map_err(LobbyError::InvalidName)?;
    let Some(session) = registry.sessions.get_mut(token) else {
        return Err(LobbyError::UnknownSession);
    };
    session.name = Some(name);
    // If the session is seated, its name appears in the shared roster, so re-project to
    // every occupant; otherwise only the sender's own view changed.
    match session.room.clone() {
        Some(room_id) => push_room(registry, &room_id),
        None => push_view(registry, token),
    }
    info!(%token, "connection set its display name");
    Ok(())
}

/// Validate a requested display name (issue #294), returning the cleaned name to
/// store or a typed [`NameError`]. Policy: trim surrounding whitespace; reject an
/// empty result, a name longer than [`MAX_NAME_LEN`] scalar values, or one holding a
/// control character (newlines, NUL, and other non-printable code points). Names need
/// not be unique — the seat's [`PlayerId`] remains the identity, so a collision is
/// allowed rather than rejected (two "Alice"s are disambiguated by their seat).
fn validate_name(requested: &str) -> Result<String, NameError> {
    let trimmed = requested.trim();
    if trimmed.is_empty() {
        return Err(NameError::Empty);
    }
    let len = trimmed.chars().count();
    if len > MAX_NAME_LEN {
        return Err(NameError::TooLong(len));
    }
    if trimmed.chars().any(char::is_control) {
        return Err(NameError::Unprintable);
    }
    Ok(trimmed.to_string())
}

/// Clear a seat's occupant and reset its pre-game gate state (a vacated seat is
/// undecked and unready). A stale room id/seat is ignored.
fn vacate(registry: &mut Registry, room_id: &RoomId, seat: usize) {
    if let Some(room) = registry.rooms.get_mut(room_id) {
        if let Some(slot) = room.seats.get_mut(seat) {
            *slot = None;
        }
        if let Some(gate) = room.gate.get_mut(seat) {
            *gate = SeatGate::default();
        }
    }
}

/// Reclaim rooms the lobby no longer needs to hold, freeing the capacity they held:
///
/// - a **pre-game** room ([`RoomEntry::game`] is `None`) with no remaining occupants
///   (every seat explicitly vacated); and
/// - a **finished** started room, whose game task has stopped so its
///   [`RoomHandle`](crate::room::RoomHandle) is no longer active (issue #280) — a live
///   game's room is kept, since its task still owns the seats' lifecycle.
fn reap_empty(registry: &mut Registry) {
    registry.rooms.retain(|room_id, room| {
        match &room.game {
            // A live game: keep it (its task owns the seats now).
            Some(handle) if handle.is_active() => true,
            // A finished game: its task has stopped, so reclaim the room.
            Some(_) => {
                info!(%room_id, "reclaimed finished room");
                false
            }
            // Pre-game: keep only while at least one seat is still occupied.
            None => {
                let occupied = room.seats.iter().any(Option::is_some);
                if !occupied {
                    info!(%room_id, "reclaimed empty room");
                }
                occupied
            }
        }
    });
}

/// The room id and seat index of a seated session, or [`LobbyError::NotSeated`] if
/// the session is not seated in a room.
fn seat_of(registry: &Registry, token: &SessionToken) -> Result<(RoomId, usize), LobbyError> {
    match registry.sessions.get(token) {
        Some(Session {
            room: Some(room_id),
            seat: Some(seat),
            ..
        }) => Ok((room_id.clone(), *seat)),
        _ => Err(LobbyError::NotSeated),
    }
}

/// Resolve a wire [`CardIdentity`] to an engine [`CardId`], or `None` if it does not
/// name a card in `db`.
///
/// A decklist names cards by their authored `functional_id` (ADR 0018 §3) — the identity
/// vocabulary ADR 0013 deferred and ADR 0018 settled. It cannot name them by `CardId`:
/// that handle is interned by `build.rs` from the catalog's sort order, so authoring one
/// new card renumbers its neighbours, and a decklist written against an integer would
/// silently come to mean different cards. The `functional_id` is the only card identity
/// stable across builds, which is exactly why it is what crosses the wire.
///
/// The wire *shape* is unchanged: [`CardIdentity`] is an opaque string the client never
/// parses, and the server remains the sole authority on what it resolves to.
///
/// [`CardIdentity`]: rune_protocol::CardIdentity
fn resolve_card(db: &CardDatabase, identity: &str) -> Option<CardId> {
    let functional_id = FunctionalId::try_from(identity.to_string()).ok()?;
    db.card_id(&functional_id)
}

/// A server-generated shuffle seed for a starting game (ADR 0012). The engine is
/// pure and takes its only randomness from this seed; the *server* is where the
/// entropy is sourced. Mixes the wall clock with a process-lifetime counter so two
/// games constructed in the same instant still get distinct seeds.
fn generate_seed() -> u64 {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    nanos ^ n.wrapping_mul(0x9E37_79B9_7F4A_7C15)
}

/// Mint an unguessable per-session token from the operating-system CSPRNG.
///
/// The token authenticates a reconnect to a held seat (issue #113), so — unlike the
/// sequential, public room id — it is a **secret** and must be neither guessable nor
/// derivable from any public value (issue #48). It carries 128 bits of entropy,
/// hex-encoded behind an `s` tag; the value is opaque to clients (`docs/protocol.md`).
///
/// # Errors
/// Propagates [`getrandom::Error`] if the OS entropy source is unavailable.
fn mint_token() -> Result<SessionToken, getrandom::Error> {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut bytes = [0u8; 16];
    getrandom::getrandom(&mut bytes)?;
    let mut token = String::with_capacity(1 + bytes.len() * 2);
    token.push('s');
    for byte in bytes {
        // Indices are always < 16, so this never panics.
        token.push(HEX[usize::from(byte >> 4)] as char);
        token.push(HEX[usize::from(byte & 0x0f)] as char);
    }
    Ok(token)
}

/// Build the [`LobbyView`] for one session, or `None` if the token is unknown.
fn build_view(registry: &Registry, token: &SessionToken) -> Option<LobbyView> {
    let session = registry.sessions.get(token)?;
    let room = session
        .room
        .as_ref()
        .and_then(|room_id| build_room_view(registry, room_id));
    let valid_commands = valid_commands(registry, session);
    Some(LobbyView {
        session: token.clone(),
        you: session.player.clone(),
        name: session.name.clone(),
        room,
        directory: build_directory(registry),
        valid_commands,
    })
}

/// Project the room registry into the public room directory (issue #280): one
/// [`RoomSummary`] per browsable room, so a connection can discover and join an open
/// game without an out-of-band id. Only the config summary, the occupancy count, and
/// the lifecycle state are exposed — never a seat roster, a decklist, or any game
/// state. The list is the same for every connection (a global browser) and sorted by
/// room id for a stable, deterministic order.
///
/// A room is `gathering` while pre-game, `in_progress` once its game has started, and
/// **omitted** once that game has ended (the room task has stopped, so its handle is
/// no longer active) — a finished room simply leaves the list.
fn build_directory(registry: &Registry) -> Vec<RoomSummary> {
    let mut directory: Vec<RoomSummary> = registry
        .rooms
        .iter()
        .filter_map(|(room_id, room)| {
            let state = match &room.game {
                None => RoomState::Gathering,
                Some(handle) if handle.is_active() => RoomState::InProgress,
                // A finished game's task has stopped: drop it from the directory.
                Some(_) => return None,
            };
            let filled = room.seats.iter().filter(|seat| seat.is_some()).count();
            Some(RoomSummary {
                room_id: room_id.clone(),
                config: room.config.clone(),
                filled: u8::try_from(filled).unwrap_or(u8::MAX),
                // The room's spectator count (ADR 0022, issue #351): observers, not
                // seats — a count only, never a spectator identity.
                spectators: u8::try_from(room.spectators.len()).unwrap_or(u8::MAX),
                state,
            })
        })
        .collect();
    directory.sort_by(|a, b| a.room_id.cmp(&b.room_id));
    directory
}

/// The lobby commands legal for a session right now — the only source of
/// interactivity in a [`LobbyView`], exactly as `valid_actions` is in `GameView`.
///
/// Roomless: `create_room`/`join_room`. Seated in a pre-game room: always
/// `submit_deck` and `leave`, plus `ready` once the seat is decked, or `unready`
/// once it is ready. (A started room's seats are on the in-game contract and never
/// see a `LobbyView`, so no in-game case appears here.)
fn valid_commands(registry: &Registry, session: &Session) -> Vec<String> {
    // `set_name` is always available in the pre-game phase (issue #294): a connection
    // may name itself before joining a room and rename at any point up to game start.
    let Some(room_id) = session.room.as_ref() else {
        return vec![
            "set_name".to_string(),
            "create_room".to_string(),
            "join_room".to_string(),
            // A roomless connection may also spectate an in-progress room (issue #351).
            "spectate_room".to_string(),
        ];
    };
    let mut commands = vec!["set_name".to_string(), "submit_deck".to_string()];
    if let (Some(room), Some(seat)) = (registry.rooms.get(room_id), session.seat) {
        if let Some(gate) = room.gate.get(seat) {
            if gate.ready {
                commands.push("unready".to_string());
            } else if gate.deck.is_some() {
                commands.push("ready".to_string());
            }
        }
    }
    commands.push("leave".to_string());
    commands
}

/// Build the [`RoomView`] for a room: its config and full seat roster, with each
/// occupant resolved to its public [`PlayerId`]. Decklist *contents* are never
/// exposed — only the derived `decked` flag and the `ready` flag per seat.
fn build_room_view(registry: &Registry, room_id: &RoomId) -> Option<RoomView> {
    let room = registry.rooms.get(room_id)?;
    let seats = room
        .seats
        .iter()
        .enumerate()
        .map(|(index, occupant)| {
            let session = occupant.as_ref().and_then(|tok| registry.sessions.get(tok));
            let occupied_by = session.map(|session| session.player.clone());
            // The occupant's chosen display name (issue #294), public in the roster.
            let name = session.and_then(|session| session.name.clone());
            let gate = room.gate.get(index);
            SeatView {
                seat: index as u8,
                occupied_by,
                name,
                decked: gate.is_some_and(|g| g.deck.is_some()),
                ready: gate.is_some_and(|g| g.ready),
            }
        })
        .collect();
    Some(RoomView {
        room_id: room_id.clone(),
        config: room.config.clone(),
        seats,
    })
}

/// Push a fresh [`LobbyView`] to one session's outbox. A closed outbox (the reader
/// is gone) is ignored — the disconnect path cleans the session up.
///
/// A session seated in a **started** room is skipped: it has already been sent the
/// terminal [`LobbySignal::Start`], and pushing a view would overwrite that hand-off
/// in the latest-value channel before the connection reads it. Started seats are on
/// the in-game contract and no longer render `LobbyView`s.
fn push_view(registry: &Registry, token: &SessionToken) {
    let Some(session) = registry.sessions.get(token) else {
        return;
    };
    if session
        .room
        .as_ref()
        .and_then(|room_id| registry.rooms.get(room_id))
        .is_some_and(|room| room.game.is_some())
    {
        return;
    }
    if let Some(view) = build_view(registry, token) {
        let _ = session.outbox.send(Some(LobbySignal::View(view)));
    }
}

/// Push a fresh [`LobbyView`] to every occupant of a room (their shared roster
/// changed).
fn push_room(registry: &Registry, room_id: &RoomId) {
    let Some(room) = registry.rooms.get(room_id) else {
        return;
    };
    let occupants: Vec<SessionToken> = room.seats.iter().flatten().cloned().collect();
    for token in &occupants {
        push_view(registry, token);
    }
}

/// Push a fresh [`LobbyView`] to **every** session, so a change to the room directory
/// (a room created, joined, left, or started — issue #280) reaches connections that
/// are browsing the room list, not just the affected room's own occupants. A session
/// seated in a started room is skipped by [`push_view`] (it is on the in-game
/// contract), so this only re-projects the directory to connections still in the
/// lobby phase.
fn broadcast_views(registry: &Registry) {
    // All borrows here are shared, so iterating the session keys while `push_view`
    // reads the registry is fine.
    for token in registry.sessions.keys() {
        push_view(registry, token);
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
                    forward_lobby_command(&lobby, &mut handle, text.as_str()).await;
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

/// Decode one JSON [`LobbyCommand`] and route it; malformed frames are logged and
/// dropped rather than closing the connection.
///
/// A `Hello` goes to [`Lobby::hello`], which may reconnect this connection to a held
/// seat and hand back a new identity — so `handle` is updated in place. Every other
/// command routes through [`Lobby::command`] against the current handle's token.
async fn forward_lobby_command(lobby: &Lobby, handle: &mut SessionHandle, text: &str) {
    match serde_json::from_str::<LobbyCommand>(text) {
        Ok(LobbyCommand::Hello(hello)) => {
            *handle = lobby.hello(handle, hello.token).await;
        }
        Ok(command) => {
            if let Err(error) = lobby.command(&handle.token, command).await {
                warn!(token = %handle.token, %error, "rejected lobby command");
            }
        }
        Err(error) => warn!(token = %handle.token, %error, "ignoring undecodable lobby command"),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use crate::test_support::fixture;

    use rune_protocol::Hello;

    fn lobby(max_rooms: usize) -> Lobby {
        Lobby::bundled(max_rooms).expect("bundled cards")
    }

    fn config(seats: u8) -> RoomConfig {
        config_with(seats, "standard_2p")
    }

    /// A room config for a specific `game_setup` format — used by the deck-legality
    /// tests, which need the strict `starter-1v1` rules (the default `standard_2p`
    /// imposes none).
    fn config_with(seats: u8, game_setup: &str) -> RoomConfig {
        RoomConfig {
            seats,
            game_setup: game_setup.to_string(),
        }
    }

    /// A legal 40-card decklist for the seeded formats, expressed as wire card
    /// identities (the server interprets each as a decimal [`CardId`]): four copies
    /// each of the five non-basics (ids 1,2,3,4,6) plus twenty basic Forests (id 5),
    /// which are exempt from the copy limit.
    fn decklist() -> Vec<String> {
        let mut cards = Vec::new();
        for slug in NON_BASICS {
            for _ in 0..4 {
                cards.push(wire_id(slug));
            }
        }
        for _ in 0..20 {
            cards.push(wire_id("forest"));
        }
        cards
    }

    /// The five non-basic cards these deck tests build with.
    const NON_BASICS: [&str; 5] = [
        "thornback_boar",
        "riverbank_otter",
        "emberfang_jackal",
        "stonehide_basilisk",
        "verdant_scout",
    ];

    /// A card as `SubmitDeck` carries it: its authored `functional_id` (ADR 0018 §3).
    fn wire_id(slug: &str) -> String {
        slug.to_string()
    }

    /// Submit a valid deck for `client`. `command` pushes synchronously, so the
    /// client's [`current`](Client::current) view reflects it once this returns.
    async fn submit_valid_deck(lobby: &Lobby, client: &Client) {
        lobby
            .command(
                &client.token,
                LobbyCommand::SubmitDeck(SubmitDeck { cards: decklist() }),
            )
            .await
            .expect("valid deck accepted");
    }

    /// A test client: a registered session plus its outbox receiver. Holds the
    /// connection `generation` too so it can build a [`SessionHandle`] for disconnect.
    struct Client {
        token: SessionToken,
        generation: u64,
        rx: watch::Receiver<Option<LobbySignal>>,
    }

    impl Client {
        async fn connect(lobby: &Lobby) -> Self {
            let (tx, rx) = watch::channel(None);
            let handle = lobby.connect(tx).await.expect("mint a session token");
            Self {
                token: handle.token,
                generation: handle.generation,
                rx,
            }
        }

        /// Simulate a returning connection that echoes `echoed` on `Hello`: a brand
        /// new socket (fresh outbox + identity) that then reconnects. The resulting
        /// client carries whatever identity the reconnect resolved to, and its
        /// receiver holds the resynced view.
        async fn reconnect(lobby: &Lobby, echoed: Option<SessionToken>) -> Self {
            let (tx, rx) = watch::channel(None);
            let fresh = lobby.connect(tx).await.expect("mint a session token");
            let adopted = lobby.hello(&fresh, echoed).await;
            Self {
                token: adopted.token,
                generation: adopted.generation,
                rx,
            }
        }

        /// The handle a real connection would present on disconnect.
        fn handle(&self) -> SessionHandle {
            SessionHandle {
                token: self.token.clone(),
                generation: self.generation,
            }
        }

        /// The latest signal pushed to this client (awaiting the next change).
        async fn signal(&mut self) -> LobbySignal {
            self.rx.changed().await.expect("a signal was pushed");
            self.rx
                .borrow_and_update()
                .clone()
                .expect("pushed signal is never the initial empty slot")
        }

        /// The latest pre-game view pushed to this client (awaiting the next change).
        async fn view(&mut self) -> LobbyView {
            match self.signal().await {
                LobbySignal::View(view) => view,
                LobbySignal::Start { .. } | LobbySignal::Spectate { .. } => {
                    panic!("expected a lobby view, got a hand-off")
                }
            }
        }

        /// The current view without waiting for a further change.
        fn current(&self) -> LobbyView {
            match self.rx.borrow().clone().expect("a signal is present") {
                LobbySignal::View(view) => view,
                LobbySignal::Start { .. } | LobbySignal::Spectate { .. } => {
                    panic!("expected a lobby view, got a hand-off")
                }
            }
        }

        /// Whether a game-start hand-off has been pushed to this client.
        fn started(&self) -> bool {
            matches!(*self.rx.borrow(), Some(LobbySignal::Start { .. }))
        }

        /// The seat carried by a pushed game-start hand-off, if any.
        fn start_seat(&self) -> Option<Seat> {
            match &*self.rx.borrow() {
                Some(LobbySignal::Start { seat, .. }) => Some(*seat),
                _ => None,
            }
        }

        /// The room handle carried by a pushed game-start hand-off, if any — the
        /// grip a determinism test uses to join the constructed game and read its
        /// first `GameView`.
        fn start_handle(&self) -> Option<RoomHandle> {
            match &*self.rx.borrow() {
                Some(LobbySignal::Start { room, .. }) => Some(room.clone()),
                _ => None,
            }
        }

        /// The room handle carried by a pushed **spectate** hand-off, if any (issue
        /// #351) — the grip a spectator test uses to join the running game as an
        /// observer and read its first `SpectatorView`.
        fn spectate_handle(&self) -> Option<RoomHandle> {
            match &*self.rx.borrow() {
                Some(LobbySignal::Spectate { room }) => Some(room.clone()),
                _ => None,
            }
        }
    }

    /// Drive two seats to a started game in a lobby pinned to the given overrides,
    /// then join seat 0 and return its first `GameView` — enough to assert the
    /// shuffle is (or is not) reproducible and the starting-life override applied,
    /// without reimplementing the engine.
    async fn first_game_view_for(
        seed_override: Option<u64>,
        life_override: Option<i32>,
    ) -> rune_protocol::GameView {
        let lobby =
            Lobby::bundled_with_overrides(4, seed_override, life_override).expect("bundled cards");
        let (alice, bob, _room) = seated_pair(&lobby).await;
        submit_valid_deck(&lobby, &alice).await;
        submit_valid_deck(&lobby, &bob).await;
        lobby
            .command(&alice.token, LobbyCommand::Ready(Ready { ready: true }))
            .await
            .expect("alice readies");
        lobby
            .command(&bob.token, LobbyCommand::Ready(Ready { ready: true }))
            .await
            .expect("bob readies");

        let handle = alice.start_handle().expect("game constructed");
        let (tx, mut rx) = watch::channel::<Option<rune_protocol::GameView>>(None);
        assert!(handle.send(crate::RoomInput::Join {
            seat: 0,
            outbox: tx
        }));
        // Await the seat's first personalized GameView.
        loop {
            if let Some(view) = rx.borrow_and_update().clone() {
                return view;
            }
            rx.changed().await.expect("first GameView is pushed");
        }
    }

    /// Seat 0's opening-hand card names for a pinned seed (no life override).
    async fn opening_hand_names_for_seed(seed_override: Option<u64>) -> Vec<String> {
        first_game_view_for(seed_override, None)
            .await
            .my_hand
            .into_iter()
            .map(|card| card.name)
            .collect()
    }

    #[tokio::test]
    async fn issue_351_a_spectator_watches_a_started_game_with_redaction_and_a_directory_count() {
        // Two players start a game; a third connection spectates it mid-game, is handed
        // off to the spectator bridge, and reads a redacted SpectatorView — while the
        // directory advertises the spectator as a count to everyone else browsing.
        let lobby = Lobby::bundled_with_overrides(8, Some(0xABCD), None).expect("bundled cards");
        let (alice, bob, room_id) = seated_pair(&lobby).await;
        submit_valid_deck(&lobby, &alice).await;
        submit_valid_deck(&lobby, &bob).await;
        lobby
            .command(&alice.token, LobbyCommand::Ready(Ready { ready: true }))
            .await
            .expect("alice readies");
        lobby
            .command(&bob.token, LobbyCommand::Ready(Ready { ready: true }))
            .await
            .expect("bob readies");
        assert!(alice.started(), "the game started");

        // A browsing client sees the room in progress with no spectators yet.
        let mut carol = Client::connect(&lobby).await;
        let dir0 = carol.view().await;
        let listed = dir0
            .directory
            .iter()
            .find(|r| r.room_id == room_id)
            .expect("the started room is listed");
        assert_eq!(listed.state, RoomState::InProgress);
        assert_eq!(listed.spectators, 0);

        // Carol spectates the in-progress room and is handed off to the spectator bridge.
        lobby
            .command(
                &carol.token,
                LobbyCommand::SpectateRoom(SpectateRoom {
                    room_id: room_id.clone(),
                }),
            )
            .await
            .expect("spectate accepted");
        let handle = carol.spectate_handle().expect("a spectate hand-off");

        // Join as a spectator and read the first redacted view.
        let (tx, mut rx) = watch::channel::<Option<rune_protocol::SpectatorView>>(None);
        assert!(handle.send(crate::RoomInput::JoinSpectator { outbox: tx }));
        let spec = loop {
            if let Some(view) = rx.borrow_and_update().clone() {
                break view;
            }
            rx.changed()
                .await
                .expect("the first SpectatorView is pushed");
        };
        // Every seat is public; there is no receiver or decision surface at all.
        assert_eq!(spec.players.len(), 2, "both seats appear as public state");
        let json = serde_json::to_value(&spec).unwrap();
        assert!(
            json.get("you").is_none(),
            "no receiver id leaks to a spectator"
        );
        assert!(
            json.get("my_hand").is_none(),
            "no hand leaks to a spectator"
        );
        assert!(
            json.get("valid_actions").is_none(),
            "no decision surface for a spectator"
        );

        // The directory now advertises the spectator (count only) to another browser.
        let mut dave = Client::connect(&lobby).await;
        let dir1 = dave.view().await;
        let watched = dir1
            .directory
            .iter()
            .find(|r| r.room_id == room_id)
            .expect("the started room is still listed");
        assert_eq!(
            watched.spectators, 1,
            "the directory advertises one spectator"
        );
    }

    #[tokio::test]
    async fn issue_351_spectating_a_gathering_room_is_rejected_non_fatally() {
        // A room that has not started has no board to watch: spectate is rejected with
        // the lobby's non-fatal error, and the would-be spectator stays roomless.
        let lobby = Lobby::bundled_with_overrides(8, None, None).expect("bundled cards");
        let (alice, _bob, room_id) = seated_pair(&lobby).await; // a gathering room
        let mut carol = Client::connect(&lobby).await;
        let _ = carol.view().await;

        let err = lobby
            .command(
                &carol.token,
                LobbyCommand::SpectateRoom(SpectateRoom {
                    room_id: room_id.clone(),
                }),
            )
            .await
            .expect_err("spectating a gathering room is rejected");
        assert_eq!(err, LobbyError::RoomNotStarted);
        // Carol is still roomless (no spectate hand-off, no seat).
        assert!(carol.spectate_handle().is_none());
        assert!(carol.current().room.is_none());
        // The seated player is unaffected.
        assert!(!alice.started());
    }

    #[tokio::test]
    async fn a_pinned_starting_life_overrides_the_format_default() {
        // Seat 0 sees seat 1 (its only opponent) start at the pinned life, not the
        // format's 20 — proof the override reaches game construction (issue #145).
        let view = first_game_view_for(Some(0xABCD), Some(4)).await;
        let opponent_life = view.opponents.first().expect("one opponent").life;
        assert_eq!(opponent_life, 4, "the starting-life override applied");
    }

    #[tokio::test]
    async fn a_pinned_seed_reproduces_the_same_opening_hand() {
        // Same override → identical shuffle (ADR 0014), so the opening hand matches.
        let first = opening_hand_names_for_seed(Some(0xC0FF_EE00_1234_5678)).await;
        let again = opening_hand_names_for_seed(Some(0xC0FF_EE00_1234_5678)).await;
        assert!(!first.is_empty(), "the opening hand is non-empty");
        assert_eq!(first, again, "a pinned seed reproduces the opening hand");

        // A different pinned seed diverges (the shuffle actually depends on it).
        let other = opening_hand_names_for_seed(Some(0x1111_2222_3333_4444)).await;
        assert_ne!(
            first, other,
            "a different seed shuffles to a different opening hand"
        );
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
    async fn create_room_seats_the_creator_and_returns_a_room_id() {
        let lobby = lobby(4);
        let mut client = Client::connect(&lobby).await;
        let initial = client.view().await;

        lobby
            .command(
                &client.token,
                LobbyCommand::CreateRoom(CreateRoom { config: config(2) }),
            )
            .await
            .expect("create a valid room");
        let view = client.view().await;

        let room = view.room.expect("creator is now in a room");
        assert!(!room.room_id.is_empty());
        assert_eq!(room.config.seats, 2);
        assert_eq!(room.seats.len(), 2);
        // The creator holds seat 0; seat 1 is empty.
        assert_eq!(
            room.seats[0].occupied_by.as_deref(),
            Some(initial.you.as_str())
        );
        assert!(room.seats[1].occupied_by.is_none());
        // No game is constructed: the roster reflects nobody decked or ready.
        assert!(room.seats.iter().all(|s| !s.decked && !s.ready));
        // Seated but undecked: the seat may submit a deck or leave, not ready up.
        assert_eq!(
            view.valid_commands,
            vec![
                "set_name".to_string(),
                "submit_deck".to_string(),
                "leave".to_string()
            ]
        );
    }

    #[tokio::test]
    async fn room_config_supports_two_through_eight_seats() {
        let lobby = lobby(8);
        for seats in SEAT_RANGE {
            let mut client = Client::connect(&lobby).await;
            let _ = client.view().await;
            lobby
                .command(
                    &client.token,
                    LobbyCommand::CreateRoom(CreateRoom {
                        config: config(seats),
                    }),
                )
                .await
                .unwrap_or_else(|_| panic!("{seats} seats is in range"));
            let room = client.view().await.room.expect("room created");
            assert_eq!(room.seats.len(), usize::from(seats));
        }
    }

    #[tokio::test]
    async fn create_room_rejects_seat_counts_outside_the_range() {
        let lobby = lobby(4);
        for seats in [0u8, 1, 9, 255] {
            let mut client = Client::connect(&lobby).await;
            let _ = client.view().await;
            let err = lobby
                .command(
                    &client.token,
                    LobbyCommand::CreateRoom(CreateRoom {
                        config: config(seats),
                    }),
                )
                .await
                .expect_err("out-of-range seat count is rejected");
            assert_eq!(err, LobbyError::InvalidSeatCount(seats));
            // Rejection re-sends the current view: still roomless.
            assert!(client.current().room.is_none());
        }
    }

    #[tokio::test]
    async fn create_room_with_an_unknown_game_setup_is_rejected() {
        // The `game_setup` id must key into the format registry (ADR 0013 §4); an
        // unknown id is refused and no room is opened.
        let lobby = lobby(4);
        let mut client = Client::connect(&lobby).await;
        let _ = client.view().await;
        let err = lobby
            .command(
                &client.token,
                LobbyCommand::CreateRoom(CreateRoom {
                    config: RoomConfig {
                        seats: 2,
                        game_setup: "no-such-format".to_string(),
                    },
                }),
            )
            .await
            .expect_err("unknown game_setup is rejected");
        assert_eq!(err, LobbyError::UnknownFormat("no-such-format".to_string()));
        // Rejection re-sends the current view: still roomless.
        assert!(client.current().room.is_none());
    }

    #[tokio::test]
    async fn create_room_accepts_the_seeded_starter_format() {
        // The seeded "starter-1v1" format resolves, so a room can be opened with it.
        let lobby = lobby(4);
        let mut client = Client::connect(&lobby).await;
        let _ = client.view().await;
        lobby
            .command(
                &client.token,
                LobbyCommand::CreateRoom(CreateRoom {
                    config: RoomConfig {
                        seats: 2,
                        game_setup: "starter-1v1".to_string(),
                    },
                }),
            )
            .await
            .expect("the seeded starter format is accepted");
        assert!(client.view().await.room.is_some());
    }

    #[tokio::test]
    async fn join_by_id_seats_the_joiner_and_updates_every_roster() {
        let lobby = lobby(4);
        let mut alice = Client::connect(&lobby).await;
        let _ = alice.view().await;
        lobby
            .command(
                &alice.token,
                LobbyCommand::CreateRoom(CreateRoom { config: config(2) }),
            )
            .await
            .expect("alice creates");
        let alice_room = alice.view().await.room.expect("alice in room");
        let room_id = alice_room.room_id.clone();

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
            .expect("bob joins by id");

        // Bob is seated at seat 1 of the same room.
        let bob_room = bob.view().await.room.expect("bob in room");
        assert_eq!(bob_room.room_id, room_id);
        assert_eq!(
            bob_room.seats[1].occupied_by.as_deref(),
            Some(bob.current().you.as_str())
        );

        // Alice was pushed an updated roster showing both seats filled.
        let alice_after = alice.view().await.room.expect("alice still in room");
        assert!(alice_after.seats[0].occupied_by.is_some());
        assert!(alice_after.seats[1].occupied_by.is_some());
    }

    #[tokio::test]
    async fn joining_an_unknown_room_is_a_typed_error() {
        let lobby = lobby(4);
        let mut bob = Client::connect(&lobby).await;
        let _ = bob.view().await;
        let err = lobby
            .command(
                &bob.token,
                LobbyCommand::JoinRoom(JoinRoom {
                    room_id: "r-nope".to_string(),
                }),
            )
            .await
            .expect_err("unknown room id is rejected");
        assert_eq!(err, LobbyError::UnknownRoom);
        assert!(bob.current().room.is_none());
    }

    #[tokio::test]
    async fn joining_a_full_room_is_a_typed_error() {
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
            .expect("bob fills the second seat");
        let _ = bob.view().await;

        // The two-seat room is full: a third joiner is refused and stays roomless.
        let mut carol = Client::connect(&lobby).await;
        let _ = carol.view().await;
        let err = lobby
            .command(&carol.token, LobbyCommand::JoinRoom(JoinRoom { room_id }))
            .await
            .expect_err("a full room is rejected");
        assert_eq!(err, LobbyError::RoomFull);
        assert!(carol.current().room.is_none());
    }

    #[tokio::test]
    async fn a_full_room_stays_in_the_lobby_phase_with_no_game() {
        // Two seats, both filled — yet with no ready gate passing nobody starts a
        // game: both occupants stay in the lobby phase. This is what retires the old
        // "live with one player and empty decks" behavior.
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

        let mut bob = Client::connect(&lobby).await;
        let _ = bob.view().await;
        lobby
            .command(&bob.token, LobbyCommand::JoinRoom(JoinRoom { room_id }))
            .await
            .unwrap();

        // Both remain in the lobby: interactivity is deck submission / leave, never
        // in-game actions. No game has been constructed.
        assert_eq!(
            bob.view().await.valid_commands,
            vec![
                "set_name".to_string(),
                "submit_deck".to_string(),
                "leave".to_string()
            ]
        );
        assert_eq!(
            alice.view().await.valid_commands,
            vec![
                "set_name".to_string(),
                "submit_deck".to_string(),
                "leave".to_string()
            ]
        );
        assert!(!bob.started() && !alice.started());
    }

    #[tokio::test]
    async fn leaving_vacates_the_seat_and_notifies_peers() {
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

        let mut bob = Client::connect(&lobby).await;
        let _ = bob.view().await;
        lobby
            .command(&bob.token, LobbyCommand::JoinRoom(JoinRoom { room_id }))
            .await
            .unwrap();
        let _ = bob.view().await;
        let _ = alice.view().await; // roster-updated push from bob's join

        // Bob leaves: his seat empties, he is roomless again, and alice is notified.
        lobby
            .command(&bob.token, LobbyCommand::Leave)
            .await
            .unwrap();
        let bob_after = bob.view().await;
        assert!(bob_after.room.is_none());
        assert_eq!(
            bob_after.valid_commands,
            vec![
                "set_name".to_string(),
                "create_room".to_string(),
                "join_room".to_string(),
                "spectate_room".to_string()
            ]
        );

        let alice_after = alice.view().await.room.expect("alice still holds the room");
        assert!(alice_after.seats[0].occupied_by.is_some());
        assert!(alice_after.seats[1].occupied_by.is_none());
    }

    #[tokio::test]
    async fn an_empty_room_is_reclaimed_and_frees_capacity() {
        // Capacity for exactly one room. Alice creates it, filling the cap; a second
        // creator is refused. Alice leaves, emptying and reclaiming the room, which
        // frees the slot for the next creator.
        let lobby = lobby(1);
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

        let mut bob = Client::connect(&lobby).await;
        let _ = bob.view().await;
        let err = lobby
            .command(
                &bob.token,
                LobbyCommand::CreateRoom(CreateRoom { config: config(2) }),
            )
            .await
            .expect_err("at capacity");
        assert_eq!(err, LobbyError::AtCapacity);

        // Alice leaves: her room is empty and reclaimed, freeing the single slot.
        lobby
            .command(&alice.token, LobbyCommand::Leave)
            .await
            .unwrap();

        // Bob can now create a room where he previously could not.
        lobby
            .command(
                &bob.token,
                LobbyCommand::CreateRoom(CreateRoom { config: config(2) }),
            )
            .await
            .expect("capacity freed by reclamation");
        assert!(bob.view().await.room.is_some());
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

    #[tokio::test]
    async fn leaving_the_last_seat_reclaims_the_room() {
        // The seat is only ever vacated by an explicit `Leave`; once the room is
        // empty it is reclaimed and its id becomes unknown.
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

        lobby
            .command(&alice.token, LobbyCommand::Leave)
            .await
            .unwrap();

        let mut carol = Client::connect(&lobby).await;
        let _ = carol.view().await;
        let err = lobby
            .command(&carol.token, LobbyCommand::JoinRoom(JoinRoom { room_id }))
            .await
            .expect_err("the reclaimed room is gone");
        assert_eq!(err, LobbyError::UnknownRoom);
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
    async fn session_tokens_are_unguessable_and_distinct_from_the_public_identity() {
        let lobby = lobby(4);
        let a = Client::connect(&lobby).await;
        let b = Client::connect(&lobby).await;

        // Per-session and unique.
        assert_ne!(a.token, b.token);
        // Real entropy, not the old sequential "s{n}" scheme an attacker could guess.
        assert!(a.token.len() >= 16, "token carries real entropy");
        assert!(
            !matches!(a.token.as_str(), "s0" | "s1" | "s2"),
            "tokens are not sequential/guessable",
        );
        // The secret token is never the public identity shown to opponents.
        assert_ne!(a.token, a.current().you, "secret token != public identity");
    }

    #[tokio::test]
    async fn create_or_join_while_already_in_a_room_is_rejected() {
        let lobby = lobby(4);
        let mut alice = Client::connect(&lobby).await;
        let _ = alice.view().await;
        lobby
            .command(
                &alice.token,
                LobbyCommand::CreateRoom(CreateRoom {
                    config: config_with(3, "standard_ffa"),
                }),
            )
            .await
            .unwrap();
        let room_id = alice.view().await.room.unwrap().room_id;

        // A second create while seated is rejected.
        assert_eq!(
            lobby
                .command(
                    &alice.token,
                    LobbyCommand::CreateRoom(CreateRoom { config: config(2) })
                )
                .await,
            Err(LobbyError::AlreadyInRoom)
        );
        // As is a join while seated.
        assert_eq!(
            lobby
                .command(&alice.token, LobbyCommand::JoinRoom(JoinRoom { room_id }))
                .await,
            Err(LobbyError::AlreadyInRoom)
        );
    }

    #[tokio::test]
    async fn commands_that_require_a_seat_are_typed_errors_when_roomless() {
        let lobby = lobby(4);
        let mut alice = Client::connect(&lobby).await;
        let _ = alice.view().await;

        // Roomless: leave, submit_deck, and ready all require a seat.
        assert_eq!(
            lobby.command(&alice.token, LobbyCommand::Leave).await,
            Err(LobbyError::NotInRoom)
        );
        assert_eq!(
            lobby
                .command(
                    &alice.token,
                    LobbyCommand::SubmitDeck(SubmitDeck::default())
                )
                .await,
            Err(LobbyError::NotSeated)
        );
        assert_eq!(
            lobby
                .command(&alice.token, LobbyCommand::Ready(Ready { ready: true }))
                .await,
            Err(LobbyError::NotSeated)
        );
    }

    /// Seat a fresh two-seat room with `alice` (creator, seat 0) and `bob` (seat 1),
    /// draining the roster pushes so each client's next `view()` is the seat's own.
    async fn seated_pair(lobby: &Lobby) -> (Client, Client, RoomId) {
        seated_pair_in(lobby, "standard_2p").await
    }

    /// Like [`seated_pair`], but opens the room under a named `game_setup` format —
    /// the deck-legality tests use `starter-1v1` so its size/copy rules apply.
    async fn seated_pair_in(lobby: &Lobby, game_setup: &str) -> (Client, Client, RoomId) {
        let mut alice = Client::connect(lobby).await;
        let _ = alice.view().await;
        lobby
            .command(
                &alice.token,
                LobbyCommand::CreateRoom(CreateRoom {
                    config: config_with(2, game_setup),
                }),
            )
            .await
            .expect("alice creates");
        let room_id = alice.view().await.room.expect("alice in room").room_id;

        let mut bob = Client::connect(lobby).await;
        let _ = bob.view().await;
        lobby
            .command(
                &bob.token,
                LobbyCommand::JoinRoom(JoinRoom {
                    room_id: room_id.clone(),
                }),
            )
            .await
            .expect("bob joins");
        let _ = bob.view().await;
        let _ = alice.view().await; // roster-updated push from bob's join
        (alice, bob, room_id)
    }

    #[tokio::test]
    async fn submit_deck_marks_the_seat_decked_for_everyone_and_offers_ready() {
        let lobby = lobby(4);
        let (alice, bob, _room) = seated_pair(&lobby).await;

        lobby
            .command(
                &alice.token,
                LobbyCommand::SubmitDeck(SubmitDeck { cards: decklist() }),
            )
            .await
            .expect("valid deck accepted");

        // Alice sees herself decked and is now offered `ready`.
        let alice_view = alice.current();
        let alice_room = alice_view.room.expect("alice in room");
        assert!(alice_room.seats[0].decked);
        assert!(!alice_room.seats[0].ready);
        assert_eq!(
            alice_view.valid_commands,
            vec![
                "set_name".to_string(),
                "submit_deck".to_string(),
                "ready".to_string(),
                "leave".to_string()
            ]
        );

        // Bob (an undecked peer) is told alice is decked, but never sees her deck.
        let bob_room = bob.current().room.expect("bob in room");
        assert!(bob_room.seats[0].decked);
        assert!(!bob_room.seats[1].decked);
    }

    #[tokio::test]
    async fn submit_deck_with_an_unknown_card_is_rejected_and_seat_stays_undecked() {
        let lobby = lobby(4);
        let (alice, _bob, _room) = seated_pair(&lobby).await;

        // A non-existent id (bundled db holds only 1..=6) rejects the whole list.
        let err = lobby
            .command(
                &alice.token,
                LobbyCommand::SubmitDeck(SubmitDeck {
                    cards: vec![wire_id("forest"), "no_such_card".to_string()],
                }),
            )
            .await
            .expect_err("unknown card id is rejected");
        assert_eq!(err, LobbyError::UnknownCard("no_such_card".to_string()));
        // The seat stays undecked; the rejection re-sent the current view.
        assert!(!alice.current().room.expect("in room").seats[0].decked);
    }

    #[tokio::test]
    async fn submit_deck_under_the_minimum_size_is_rejected_and_seat_stays_undecked() {
        // The seeded format requires 40 cards (ADR 0013 §4); a ten-card deck of known
        // ids is rejected as illegal, and the seat is left undecked.
        let lobby = lobby(4);
        let (alice, _bob, _room) = seated_pair_in(&lobby, "starter-1v1").await;

        let err = lobby
            .command(
                &alice.token,
                LobbyCommand::SubmitDeck(SubmitDeck {
                    cards: vec![wire_id("forest"); 10],
                }),
            )
            .await
            .expect_err("an under-minimum deck is rejected");
        assert_eq!(
            err,
            LobbyError::IllegalDeck(DeckError::BelowMinimum { have: 10, min: 40 })
        );
        assert!(!alice.current().room.expect("in room").seats[0].decked);
    }

    #[tokio::test]
    async fn submit_deck_over_the_copy_limit_for_a_non_basic_is_rejected() {
        // Five copies of a non-basic (id 1) in an otherwise legal 40-card deck exceed
        // the four-copy limit (ADR 0013 §4); the deck is rejected and stays out.
        let lobby = lobby(4);
        let (alice, _bob, _room) = seated_pair_in(&lobby, "starter-1v1").await;

        let mut cards = vec![wire_id("thornback_boar"); 5];
        for slug in &NON_BASICS[1..] {
            for _ in 0..4 {
                cards.push(wire_id(slug));
            }
        }
        for _ in 0..19 {
            cards.push(wire_id("forest"));
        }
        assert_eq!(cards.len(), 40);

        let err = lobby
            .command(&alice.token, LobbyCommand::SubmitDeck(SubmitDeck { cards }))
            .await
            .expect_err("an over-copy-limit deck is rejected");
        assert_eq!(
            err,
            LobbyError::IllegalDeck(DeckError::CopyLimit {
                card: fixture("thornback_boar"),
                count: 5,
                limit: 4,
            })
        );
        assert!(!alice.current().room.expect("in room").seats[0].decked);
    }

    #[tokio::test]
    async fn submit_deck_accepts_a_legal_deck_with_many_basics() {
        // The shared `decklist()` holds twenty basic Forests, far over the
        // four-copy limit, yet basics are exempt (ADR 0013 §4): the deck is accepted.
        let lobby = lobby(4);
        let (alice, _bob, _room) = seated_pair_in(&lobby, "starter-1v1").await;

        lobby
            .command(
                &alice.token,
                LobbyCommand::SubmitDeck(SubmitDeck { cards: decklist() }),
            )
            .await
            .expect("a legal deck with many basics is accepted");
        assert!(alice.current().room.expect("in room").seats[0].decked);
    }

    #[tokio::test]
    async fn readying_up_requires_a_submitted_deck() {
        let lobby = lobby(4);
        let (alice, _bob, _room) = seated_pair(&lobby).await;

        // Ready before decking is a typed error; the seat stays unready.
        assert_eq!(
            lobby
                .command(&alice.token, LobbyCommand::Ready(Ready { ready: true }))
                .await,
            Err(LobbyError::NotDecked)
        );
        assert!(!alice.current().room.expect("in room").seats[0].ready);
    }

    #[tokio::test]
    async fn ready_toggles_and_un_ready_is_allowed_before_start() {
        // `command` pushes synchronously, so `current()` reflects the latest state
        // as soon as the call returns — no need to await intermediate frames (which
        // a latest-value watch would coalesce anyway).
        let lobby = lobby(4);
        let (alice, bob, _room) = seated_pair(&lobby).await;
        submit_valid_deck(&lobby, &alice).await;

        // Ready up: alice's seat shows ready and she is now offered `unready`, and
        // her peer sees the flag too.
        lobby
            .command(&alice.token, LobbyCommand::Ready(Ready { ready: true }))
            .await
            .expect("ready accepted");
        assert!(alice.current().room.expect("in room").seats[0].ready);
        assert_eq!(
            alice.current().valid_commands,
            vec![
                "set_name".to_string(),
                "submit_deck".to_string(),
                "unready".to_string(),
                "leave".to_string()
            ]
        );
        assert!(bob.current().room.expect("in room").seats[0].ready);

        // Un-ready: allowed before the game starts; the flag clears for everyone.
        lobby
            .command(&alice.token, LobbyCommand::Ready(Ready { ready: false }))
            .await
            .expect("un-ready accepted");
        assert!(!alice.current().room.expect("in room").seats[0].ready);
        assert!(!bob.current().room.expect("in room").seats[0].ready);
        // Only the decked seat readied then un-readied: still no game.
        assert!(!alice.started() && !bob.started());
    }

    #[tokio::test]
    async fn start_is_blocked_while_a_seat_is_undecked() {
        let lobby = lobby(4);
        let (alice, bob, _room) = seated_pair(&lobby).await;
        // Only alice decks and readies; bob never submits a deck.
        submit_valid_deck(&lobby, &alice).await;
        lobby
            .command(&alice.token, LobbyCommand::Ready(Ready { ready: true }))
            .await
            .expect("alice readies");

        // The gate cannot pass with bob undecked: no game is constructed.
        assert!(!alice.started() && !bob.started());
        assert!(alice.current().room.expect("in room").seats[0].ready);
    }

    #[tokio::test]
    async fn start_is_blocked_while_a_seat_is_unready() {
        let lobby = lobby(4);
        let (alice, bob, _room) = seated_pair(&lobby).await;
        // Both deck; only alice readies.
        submit_valid_deck(&lobby, &alice).await;
        submit_valid_deck(&lobby, &bob).await;
        lobby
            .command(&alice.token, LobbyCommand::Ready(Ready { ready: true }))
            .await
            .expect("alice readies");

        // Bob is decked but unready: the gate stays shut.
        assert!(!alice.started() && !bob.started());
    }

    #[tokio::test]
    async fn last_seat_readying_constructs_the_game_and_hands_off_every_seat() {
        let lobby = lobby(4);
        let (alice, bob, _room) = seated_pair(&lobby).await;
        submit_valid_deck(&lobby, &alice).await;
        submit_valid_deck(&lobby, &bob).await;

        // Alice readies first — not enough; the gate needs every seat.
        lobby
            .command(&alice.token, LobbyCommand::Ready(Ready { ready: true }))
            .await
            .expect("alice readies");
        assert!(!alice.started() && !bob.started());

        // Bob readies last: the gate passes and both seats are handed off to a game.
        // The terminal `Start` supersedes the roster push in each latest-value outbox.
        lobby
            .command(&bob.token, LobbyCommand::Ready(Ready { ready: true }))
            .await
            .expect("bob readies");

        assert_eq!(alice.start_seat(), Some(0));
        assert_eq!(bob.start_seat(), Some(1));

        // Post-start, further lobby commands to the started room are rejected.
        assert_eq!(
            lobby
                .command(&alice.token, LobbyCommand::Ready(Ready { ready: false }))
                .await,
            Err(LobbyError::GameStarted)
        );
    }

    #[tokio::test]
    async fn issue_349_ffa_format_rejects_a_seat_count_it_does_not_allow() {
        // The free-for-all format seats 3–4 (issue #349): a two-seat request is a
        // valid lobby seat count but not one this format allows, so it is rejected
        // non-fatally and no room opens.
        let lobby = lobby(4);
        let mut client = Client::connect(&lobby).await;
        let _ = client.view().await;
        let err = lobby
            .command(
                &client.token,
                LobbyCommand::CreateRoom(CreateRoom {
                    config: config_with(2, "standard_ffa"),
                }),
            )
            .await
            .expect_err("2 seats is not a free-for-all count");
        assert_eq!(
            err,
            LobbyError::SeatCountForFormat {
                seats: 2,
                format: "standard_ffa".to_string(),
            }
        );
        assert!(client.current().room.is_none());
    }

    #[tokio::test]
    async fn issue_349_three_seat_free_for_all_starts_a_three_player_game() {
        // Creating a 3-seat free-for-all room, decking and readying every seat, starts
        // an engine game seating that many players (the FFA-format acceptance).
        let lobby = lobby(4);
        let mut alice = Client::connect(&lobby).await;
        let _ = alice.view().await;
        lobby
            .command(
                &alice.token,
                LobbyCommand::CreateRoom(CreateRoom {
                    config: config_with(3, "standard_ffa"),
                }),
            )
            .await
            .expect("alice creates a 3-seat FFA room");
        let room_id = alice.view().await.room.expect("alice in room").room_id;
        assert_eq!(
            alice.current().room.unwrap().seats.len(),
            3,
            "the room has three seats"
        );

        // Two more players join.
        let mut others = Vec::new();
        for _ in 0..2 {
            let mut client = Client::connect(&lobby).await;
            let _ = client.view().await;
            lobby
                .command(
                    &client.token,
                    LobbyCommand::JoinRoom(JoinRoom {
                        room_id: room_id.clone(),
                    }),
                )
                .await
                .expect("player joins the FFA room");
            let _ = client.view().await;
            others.push(client);
        }
        let _ = alice.view().await;

        // Every seat decks and readies; the game starts only once all three are in.
        submit_valid_deck(&lobby, &alice).await;
        for client in &others {
            submit_valid_deck(&lobby, client).await;
        }
        lobby
            .command(&alice.token, LobbyCommand::Ready(Ready { ready: true }))
            .await
            .expect("alice readies");
        for client in &others {
            lobby
                .command(&client.token, LobbyCommand::Ready(Ready { ready: true }))
                .await
                .expect("player readies");
        }

        // All three seats are handed off to a running game, one per seat index.
        assert_eq!(alice.start_seat(), Some(0));
        assert_eq!(others[0].start_seat(), Some(1));
        assert_eq!(others[1].start_seat(), Some(2));
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

    #[tokio::test]
    async fn set_name_is_accepted_and_projects_into_the_lobby_and_roster() {
        // Issue #294: a chosen name lands on the connection's own view and, once seated,
        // in the shared roster every occupant sees.
        let lobby = lobby(4);
        let (mut alice, mut bob, _room) = seated_pair(&lobby).await;

        lobby
            .command(
                &alice.token,
                LobbyCommand::SetName(SetName {
                    name: "Alice".into(),
                }),
            )
            .await
            .expect("a valid name is accepted");

        // Alice's own view carries her name...
        assert_eq!(alice.view().await.name.as_deref(), Some("Alice"));
        // ...and the roster names her seat for the peer (a public, un-redacted field).
        let bob_room = bob.view().await.room.expect("bob in room");
        assert_eq!(bob_room.seats[0].name.as_deref(), Some("Alice"));
        assert_eq!(bob_room.seats[1].name, None, "bob has not named himself");
    }

    #[tokio::test]
    async fn set_name_trims_and_rejects_invalid_names_non_fatally() {
        // Issue #294: validation policy — trim; reject empty/whitespace, over-long, and
        // control-character names with a typed error, leaving the stored name untouched
        // (the lobby's non-fatal pattern; the caller re-sends the current view).
        let lobby = lobby(4);
        let mut alice = Client::connect(&lobby).await;
        let _ = alice.view().await;

        // A surrounding-whitespace name is trimmed, not rejected.
        lobby
            .command(
                &alice.token,
                LobbyCommand::SetName(SetName {
                    name: "  Alice  ".into(),
                }),
            )
            .await
            .expect("a trimmable name is accepted");
        assert_eq!(alice.view().await.name.as_deref(), Some("Alice"));

        // Empty / whitespace-only.
        assert_eq!(
            lobby
                .command(
                    &alice.token,
                    LobbyCommand::SetName(SetName { name: "   ".into() })
                )
                .await,
            Err(LobbyError::InvalidName(NameError::Empty))
        );
        // Over the length limit (counted in scalar values).
        let too_long = "x".repeat(MAX_NAME_LEN + 1);
        assert_eq!(
            lobby
                .command(
                    &alice.token,
                    LobbyCommand::SetName(SetName { name: too_long })
                )
                .await,
            Err(LobbyError::InvalidName(NameError::TooLong(
                MAX_NAME_LEN + 1
            )))
        );
        // A control character (newline) is non-printable.
        assert_eq!(
            lobby
                .command(
                    &alice.token,
                    LobbyCommand::SetName(SetName {
                        name: "Al\nice".into()
                    })
                )
                .await,
            Err(LobbyError::InvalidName(NameError::Unprintable))
        );

        // The earlier accepted name is untouched by the rejected attempts.
        assert_eq!(alice.current().name.as_deref(), Some("Alice"));
    }

    #[tokio::test]
    async fn a_display_name_survives_a_reconnect() {
        // Issue #294: the name is bound to the session, so a per-tab reconnect (echoing
        // the session token) is reunited with the same name.
        let lobby = lobby(4);
        let (alice, _bob, _room) = seated_pair(&lobby).await;
        lobby
            .command(
                &alice.token,
                LobbyCommand::SetName(SetName {
                    name: "Alice".into(),
                }),
            )
            .await
            .expect("name accepted");

        // Drop the connection (the seated session is held open) and reconnect by token.
        lobby.disconnect(&alice.handle()).await;
        let mut returning = Client::reconnect(&lobby, Some(alice.token.clone())).await;
        let resumed = returning.view().await;
        assert_eq!(
            resumed.name.as_deref(),
            Some("Alice"),
            "name survived reconnect"
        );
        let room = resumed.room.expect("reclaimed the held seat");
        assert_eq!(room.seats[0].name.as_deref(), Some("Alice"));
    }

    #[tokio::test]
    async fn player_names_project_into_the_game_view_at_game_start() {
        // Issue #294: names set in the lobby reach the constructed game, keyed by the
        // `p{N}` player id, so every in-game surface can label players.
        let lobby = lobby(4);
        let (alice, bob, _room) = seated_pair(&lobby).await;
        lobby
            .command(
                &alice.token,
                LobbyCommand::SetName(SetName {
                    name: "Alice".into(),
                }),
            )
            .await
            .expect("alice names herself");
        lobby
            .command(
                &bob.token,
                LobbyCommand::SetName(SetName { name: "Bob".into() }),
            )
            .await
            .expect("bob names himself");
        submit_valid_deck(&lobby, &alice).await;
        submit_valid_deck(&lobby, &bob).await;
        lobby
            .command(&alice.token, LobbyCommand::Ready(Ready { ready: true }))
            .await
            .expect("alice readies");
        lobby
            .command(&bob.token, LobbyCommand::Ready(Ready { ready: true }))
            .await
            .expect("bob readies");

        // Join seat 0's constructed game and read its first personalized GameView.
        let handle = alice.start_handle().expect("game constructed");
        let (tx, mut rx) = watch::channel::<Option<rune_protocol::GameView>>(None);
        assert!(handle.send(crate::RoomInput::Join {
            seat: 0,
            outbox: tx
        }));
        let view = loop {
            if let Some(view) = rx.borrow_and_update().clone() {
                break view;
            }
            rx.changed().await.expect("first GameView is pushed");
        };
        assert_eq!(
            view.player_names.get("p0").map(String::as_str),
            Some("Alice")
        );
        assert_eq!(view.player_names.get("p1").map(String::as_str), Some("Bob"));
    }
}
