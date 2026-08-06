//! Layer 1 lobby — session identity, the explicit-room registry, the pre-game
//! `LobbyView`/`LobbyCommand` routing (issue #110), the deck-submission and
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
//! There is deliberately **no auto-seating** and no matchmaking. A
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
//! behavior.
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
//! - [`commands`] — the command handlers (`submit_deck`, `ready`, `start_game`, and
//!   the `join`/`spectate`/`leave`/`set_name`/AI-seat routing).
//! - [`room_config`] — the two commands that own a room's [`RoomConfig`]:
//!   `create_room` and the host-only `update_room` (issue #546).
//! - [`views`] — building and pushing the `LobbyView`/directory/room roster.
//! - [`registry`] — registry and session helpers (seat/room lookup, card and name
//!   validation, seed/token minting).

use std::collections::HashMap;
use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use futures_util::{SinkExt, StreamExt};
use sage_engine::{
    CardDatabase, CardId, CatalogError, FunctionalId, GameSetup, GameState, PlayerSetup,
};
use sage_protocol::{
    AddAi, CatalogView, CreateRoom, JoinRoom, LobbyCommand, LobbyView, MatchFormat, PlayerId,
    Ready, RemoveAi, RoomConfig, RoomId, RoomState, RoomSummary, RoomView, RoomVisibility,
    SeatView, SessionToken, SetName, SpectateRoom, SubmitDeck, UpdateRoom,
};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::{watch, RwLock};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;
use tracing::{info, warn};

use crate::ai::{policy_for, serve_ai_seat, AiKind};
use crate::format::{DeckError, FormatRegistry};
use crate::room::{
    serve_connection, serve_spectator_connection, AutoPassPolicy, Room, RoomHandle, Seat,
    StopPolicy,
};

mod commands;
mod connection;
mod errors;
mod registry;
mod room_config;
mod views;

#[cfg(test)]
pub(crate) mod test_support;

pub use connection::serve_lobby_connection;

// Everything the submodules above reach through `crate::lobby::*`. The allow is load
// bearing: rustc's unused-import lint counts a *named* consumer, and since the split
// (issue #711) every consumer of these is another glob, so without it a re-export the
// build genuinely needs reads as dead.
#[allow(unused_imports)]
pub(crate) use {commands::*, errors::*, registry::*, views::*};

/// Inclusive range of seats a room may be configured with. The lobby and room
/// plumbing support 2–8 seats even while the engine remains two-player:
/// a config the engine cannot yet build a game for is caught later, at the ready
/// gate (issue #112), not here.
/// `pub(crate)` because the format registry's ranges are judged against it: every
/// registered format seats a subset of what the lobby plumbs (issue #707).
pub(crate) const SEAT_RANGE: std::ops::RangeInclusive<u8> = 2..=8;

/// The maximum length (in Unicode scalar values) of a player display name (issue
/// #294). Long enough for real names/handles, short enough to keep rosters and
/// in-game labels readable and bound the stored/echoed string. Counts `char`s, not
/// bytes, so a multi-byte name is judged by what a reader sees.
const MAX_NAME_LEN: usize = 32;

/// What the lobby pushes to one connection: either a fresh full [`LobbyView`] to
/// render, or — the instant the ready gate passes — the hand-off that switches the
/// connection to the in-game contract.
///
/// Not a protocol type: it never touches the wire. The connection task
/// ([`serve_lobby_connection`]) serializes a [`View`](LobbySignal::View) to JSON and
/// writes it back, and on [`Start`](LobbySignal::Start) it reunites its socket and
/// hands off to [`serve_connection`], after which the room speaks `GameView`s.
#[derive(Clone)]
pub(crate) enum LobbySignal {
    /// A fresh pre-game snapshot to serialize and send. Boxed because a full
    /// `LobbyView` dwarfs the two hand-off variants, and every push would otherwise
    /// carry that footprint through the channel.
    View(Box<LobbyView>),
    /// The gate passed: this connection now owns `seat` of a started room and
    /// should switch to the in-game contract driven by `room`.
    Start {
        /// The seat this connection holds at the table.
        seat: Seat,
        /// Handle to the running room task that now owns the one game.
        room: RoomHandle,
    },
    /// This connection joined as a **spectator** (issue #351): it should
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
    ///.
    ///
    /// [`CardIdentity`]: sage_protocol::CardIdentity
    db: CardDatabase,
    /// The server's format registry: each room's `game_setup` id is a
    /// key into this, yielding the engine [`GameSetup`] the room starts with plus the
    /// deck-legality rules [`Lobby::submit_deck`] validates a decklist against. A
    /// `CreateRoom` naming an unknown id is rejected before a room opens. Deck
    /// legality is *server* policy, never an engine rule.
    formats: FormatRegistry,
    /// The cap on concurrently hosted rooms.
    max_rooms: usize,
    /// A fixed engine shuffle seed to build every game from, when set. `None` for
    /// normal play, where [`start_game`](Lobby::start_game) generates a distinct
    /// per-game seed. The seed is server-side state that never reaches a client
    /// (ADR 0006); pinning it makes a whole game reproducible for the end-to-end
    /// suite (issue #145). Sourced from [`Config::rng_seed`](crate::Config::rng_seed).
    seed_override: Option<u64>,
    /// A fixed starting life total to build every game from, when set, overriding
    /// the room format's default. `None` for normal play. A low value
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
/// to the in-game contract (issue #112).
struct RoomEntry {
    /// The room's configuration, echoed in every [`RoomView`].
    config: RoomConfig,
    /// Per-seat occupancy: the [`SessionToken`] seated at each index, or `None`.
    seats: Vec<Option<SessionToken>>,
    /// Per-seat **AI opponent** occupancy (issue #415), parallel to
    /// [`seats`](RoomEntry::seats): `Some` when the host has filled that seat with an AI,
    /// `None` for a human-or-empty seat. A seat is *occupied* when either vector holds a
    /// value at its index; a seat is *free to join* only when both are `None`. An AI seat's
    /// submitted deck lives in the parallel [`gate`](RoomEntry::gate) (decked + ready), just
    /// like a human's, so the ready gate treats it uniformly.
    ai_seats: Vec<Option<AiSeat>>,
    /// Per-seat gate state (submitted deck + ready flag), parallel to
    /// [`seats`](RoomEntry::seats). Kept in a separate vector so seat *occupancy*
    /// stays a plain `Vec<Option<SessionToken>>`.
    gate: Vec<SeatGate>,
    /// The running game once the ready gate has passed; `None` while the room is
    /// still pre-game. A started room is never reaped as "empty" and rejects further
    /// lobby commands — its seats speak `GameView`s now.
    game: Option<RoomHandle>,
    /// The sessions currently **spectating** this room (issue #351). A
    /// spectator does not occupy a seat, so this is separate from
    /// [`seats`](RoomEntry::seats): the directory advertises `spectators.len()` as the
    /// room's spectator count, independent of seat occupancy. Spectating only starts
    /// once the room's [`game`](RoomEntry::game) is running (there is no pre-game board
    /// to watch), and a spectator is removed on `leave` or disconnect.
    spectators: Vec<SessionToken>,
}

/// A seat filled with an **AI opponent** (issue #415): the kind of AI playing it and the
/// public label it shows in the roster and in-game. The seat's deck lives in the parallel
/// [`SeatGate`] (chosen by the host at [`AddAi`] time), so an AI seat is decked + ready by
/// construction and passes the ready gate exactly like a human.
#[derive(Clone)]
struct AiSeat {
    /// Which AI policy plays this seat (dispatched by [`policy_for`] at game start).
    kind: AiKind,
    /// The public display name for the AI, projected into [`SeatView::name`] and, once the
    /// game starts, every `GameView::player_names` — so an opponent reads "Random" rather
    /// than a bare seat label.
    name: String,
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
    /// What the seat shows the table about the deck it submitted: the colours it is in
    /// (WUBRG) and the `functional_id` of its commander.
    ///
    /// Derived once, where the deck is accepted, rather than in the view — the view
    /// builder reads the registry and has no card database, and a summary computed on
    /// every broadcast would be recomputing a constant. Cleared with the deck it
    /// describes, so it can never outlive it.
    shown: SeatShown,
    /// Whether the seat has declared itself ready. A seat may ready only once
    /// [`deck`](SeatGate::deck) is `Some`.
    ready: bool,
}

/// The public summary of a seat's deck: what a player at the table can see of it.
#[derive(Clone, Default)]
struct SeatShown {
    /// The deck's colour identity in WUBRG order, empty when the seat has no deck.
    colors: Vec<sage_protocol::Color>,
    /// The designated commander's `functional_id`, if the seat designated one.
    commander: Option<String>,
}

impl Lobby {
    /// The default cap on concurrently hosted rooms. Kept modest and explicit for
    /// now; real capacity planning is a later concern.
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
    /// (ADR 0006) and/or a fixed `life_override` starting life every game is built
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

    /// Project a rejected command's [`LobbyError`] into the structured, human-readable
    /// [`LobbyRejection`] the serve loop delivers to the **rejecting connection only**
    /// (issue #395), or `None` when the rejection carries no deck reason worth naming.
    ///
    /// Deck-content rejections are surfaced this way: an illegal decklist
    /// ([`LobbyError::IllegalDeck`], with the structured [`DeckError`] reason) and an
    /// unresolvable card identity ([`LobbyError::UnknownCard`]). Both name a card from
    /// the **sender's own** submission, so nothing about another seat's deck or hidden
    /// state can leak.
    ///
    /// So are the **table-configuration** rejections (issue #546), which ride the same
    /// channel rather than inventing a second one: a rejected `create_room`/`update_room`
    /// otherwise changes nothing observable, so without a reason the host would watch an
    /// Edit Table press do nothing at all. Each reports only what the sender itself sent
    /// — a seat count, a format id, its own table name, its own room's occupancy — so
    /// there is nothing to leak. Every other rejection returns `None`; the client still
    /// infers a generic retry hint from the unchanged re-sent view.
    pub(crate) fn deck_rejection(
        &self,
        error: &LobbyError,
    ) -> Option<sage_protocol::LobbyRejection> {
        /// A rejection with no offending card — the shape every config rejection takes.
        fn config(code: &str, reason: String) -> Option<sage_protocol::LobbyRejection> {
            Some(sage_protocol::LobbyRejection {
                code: code.to_string(),
                reason,
                card: None,
            })
        }
        match error {
            LobbyError::IllegalDeck(deck_error) => Some(deck_error.to_rejection(&self.inner.db)),
            LobbyError::UnknownCard(identity) => Some(sage_protocol::LobbyRejection {
                code: "unknown_card".to_string(),
                reason: format!("unknown card identity {identity}"),
                card: Some(identity.clone()),
            }),
            LobbyError::InvalidSeatCount(_) => config("invalid_seat_count", error.to_string()),
            LobbyError::SeatCountForFormat { .. } => {
                config("seat_count_for_format", error.to_string())
            }
            LobbyError::UnknownFormat(_) => config("unknown_format", error.to_string()),
            LobbyError::SeatsBelowOccupancy { .. } => {
                config("seats_below_occupancy", error.to_string())
            }
            LobbyError::InvalidRoomName(_) => config("invalid_room_name", error.to_string()),
            LobbyError::NotHost => config("not_host", error.to_string()),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests;
