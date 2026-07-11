//! Layer 1 lobby — session identity plus the explicit-room registry and the
//! pre-game `LobbyView`/`LobbyCommand` routing (ADR 0012, issue #110).
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
//! # Explicit rooms — create with config, join by id
//! There is deliberately **no auto-seating** and no matchmaking (ADR 0012). A
//! connection either *creates* a room with a [`RoomConfig`] (a seat count in
//! `2..=8`) — receiving a shareable [`RoomId`] — or *joins* an existing room whose
//! id it was given. Joining a full or unknown room is a typed [`LobbyError`]; the
//! connection's current [`LobbyView`] is re-sent, exactly as an illegal
//! `ChooseAction` re-sends the current `GameView` (`docs/protocol.md`).
//!
//! # No game until the pre-game gate passes
//! Creating or joining a room does **not** construct an engine game or send a
//! `GameView`. A room stays in the lobby phase — pushing `LobbyView`s — until every
//! seat is filled, decked, and ready (the ready gate, issue #112). This retires the
//! previous "auto-seat into a game that is already live with one player and empty
//! decks" behavior (ADR 0012).
//!
//! # Holding seats for reconnect, and reclaiming rooms
//! A **seated** session is held open across a dropped connection: a disconnect
//! neither vacates the seat nor reclaims the room, so the session's token can later
//! reclaim exactly that seat (issue #113). This mirrors the room task's own
//! seat-holding policy (`room.rs`) and, like it, has no idle timeout yet (turn
//! clocks are a later milestone). A **roomless** session holds nothing to reconnect
//! to, so it is dropped outright on disconnect. A room's registry entry — and the
//! [`Lobby::max_rooms`] capacity it holds — is reclaimed once the room is **empty**,
//! i.e. every seat has been *explicitly* vacated by a `Leave`. Reclamation runs
//! opportunistically on room creation (so freed capacity is available to the next
//! creator, even at the cap) and after every leave. Migrated from the
//! game-over/abandonment reaping of issue #54, which reaped *game-task* rooms.
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
use std::sync::Arc;

use futures_util::{SinkExt, StreamExt};
use rune_engine::CardDatabase;
use rune_protocol::{
    CreateRoom, JoinRoom, LobbyCommand, LobbyView, PlayerId, RoomConfig, RoomId, RoomView,
    SeatView, SessionToken,
};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::{watch, RwLock};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;
use tracing::{info, warn};

/// Inclusive range of seats a room may be configured with. The lobby and room
/// plumbing support 2–8 seats even while the engine remains two-player (ADR 0012):
/// a config the engine cannot yet build a game for is caught later, at the ready
/// gate (issue #112), not here.
const SEAT_RANGE: std::ops::RangeInclusive<u8> = 2..=8;

/// Latest-value outbox the lobby pushes a connection's [`LobbyView`] to. Like the
/// room's per-seat outbox, it is a [`watch`] so a slow reader always observes the
/// newest lobby state and never accumulates a backlog of superseded snapshots.
pub(crate) type LobbyOutbox = watch::Sender<Option<LobbyView>>;

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
    /// against. Not yet consumed here: the ready gate (issue #112) constructs the
    /// game and validates submitted decks against it. Held now because the lobby
    /// owns the database every room draws from (ADR 0012).
    #[allow(dead_code)]
    db: CardDatabase,
    /// The cap on concurrently hosted rooms.
    max_rooms: usize,
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
    /// The room this session currently occupies, if any.
    room: Option<RoomId>,
    /// The seat index within [`Session::room`], if seated.
    seat: Option<usize>,
    /// Where this connection's [`LobbyView`]s are pushed. After a disconnect of a
    /// held (seated) session the receiver is gone, so pushes silently no-op until a
    /// reconnect installs a fresh outbox here.
    outbox: LobbyOutbox,
    /// The generation of the connection currently attached to this session. Bumped
    /// on every (re)attach so a stale, superseded connection's teardown is ignored.
    generation: u64,
}

/// One pre-game room: a config plus a per-seat occupancy roster. It holds **no**
/// engine game — that is constructed only when the ready gate passes (issue #112).
struct RoomEntry {
    /// The room's configuration, echoed in every [`RoomView`].
    config: RoomConfig,
    /// Per-seat occupancy: the [`SessionToken`] seated at each index, or `None`.
    seats: Vec<Option<SessionToken>>,
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
    /// `join_room` with an id no active room has.
    UnknownRoom,
    /// `join_room` on a room whose every seat is occupied.
    RoomFull,
    /// `create_room` while the registry is already at [`Lobby::max_rooms`].
    AtCapacity,
    /// A protocol command not yet handled in this phase (`submit_deck`, `ready`):
    /// the deck/ready gate is issue #112.
    Unsupported,
}

impl std::fmt::Display for LobbyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownSession => write!(f, "unknown session"),
            Self::AlreadyInRoom => write!(f, "already in a room"),
            Self::NotInRoom => write!(f, "not in a room"),
            Self::InvalidSeatCount(n) => write!(f, "seat count {n} is outside 2..=8"),
            Self::UnknownRoom => write!(f, "unknown room id"),
            Self::RoomFull => write!(f, "room is full"),
            Self::AtCapacity => write!(f, "lobby is at room capacity"),
            Self::Unsupported => write!(f, "command not supported in this phase yet"),
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
    /// most `max_rooms` rooms at once.
    #[must_use]
    pub fn new(db: CardDatabase, max_rooms: usize) -> Self {
        Self {
            inner: Arc::new(Inner {
                registry: RwLock::new(Registry::default()),
                db,
                max_rooms,
            }),
        }
    }

    /// Create a lobby whose rooms use the engine's bundled card database.
    ///
    /// # Errors
    /// Returns the underlying [`serde_json::Error`] if the bundled snapshot fails
    /// to parse (see [`CardDatabase::bundled`]).
    pub fn bundled(max_rooms: usize) -> Result<Self, serde_json::Error> {
        Ok(Self::new(CardDatabase::bundled()?, max_rooms))
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
            LobbyCommand::Leave => leave_room(&mut registry, token),
            // The deck-submission and ready gate is issue #112.
            LobbyCommand::SubmitDeck(_) | LobbyCommand::Ready(_) => Err(LobbyError::Unsupported),
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
        // Free capacity held by empty rooms before checking the cap, so a creator is
        // never refused for a slot no live room still needs.
        reap_empty(registry);
        if registry.rooms.len() >= self.inner.max_rooms {
            return Err(LobbyError::AtCapacity);
        }

        let n = registry.next_room;
        registry.next_room += 1;
        let room_id = format!("r{n}");
        let mut seats = vec![None; config.seats as usize];
        seats[0] = Some(token.clone());
        registry
            .rooms
            .insert(room_id.clone(), RoomEntry { config, seats });
        if let Some(session) = registry.sessions.get_mut(token) {
            session.room = Some(room_id.clone());
            session.seat = Some(0);
        }
        info!(%token, %room_id, "opened room");
        Ok(())
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
    // Every occupant's roster changed: push all of them a fresh view.
    push_room(registry, room_id);
    info!(%token, %room_id, seat, "joined room");
    Ok(())
}

/// Handle `leave`: vacate the sender's seat, reclaim the room if it is now empty,
/// otherwise notify the remaining occupants.
fn leave_room(registry: &mut Registry, token: &SessionToken) -> Result<(), LobbyError> {
    let (room_id, seat) = match registry.sessions.get(token) {
        Some(Session {
            room: Some(room_id),
            seat: Some(seat),
            ..
        }) => (room_id.clone(), *seat),
        _ => return Err(LobbyError::NotInRoom),
    };
    vacate(registry, &room_id, seat);
    if let Some(session) = registry.sessions.get_mut(token) {
        session.room = None;
        session.seat = None;
    }
    reap_empty(registry);
    if registry.rooms.contains_key(&room_id) {
        push_room(registry, &room_id);
    }
    info!(%token, %room_id, seat, "left room");
    Ok(())
}

/// Clear a seat's occupant. A stale room id/seat is ignored.
fn vacate(registry: &mut Registry, room_id: &RoomId, seat: usize) {
    if let Some(room) = registry.rooms.get_mut(room_id) {
        if let Some(slot) = room.seats.get_mut(seat) {
            *slot = None;
        }
    }
}

/// Drop every room with no remaining occupants, freeing the capacity it held.
fn reap_empty(registry: &mut Registry) {
    registry.rooms.retain(|room_id, room| {
        let occupied = room.seats.iter().any(Option::is_some);
        if !occupied {
            info!(%room_id, "reclaimed empty room");
        }
        occupied
    });
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
    // `valid_commands` is the only source of interactivity; advertise exactly what
    // this phase implements. The deck/ready commands arrive with issue #112.
    let valid_commands = if session.room.is_some() {
        vec!["leave".to_string()]
    } else {
        vec!["create_room".to_string(), "join_room".to_string()]
    };
    Some(LobbyView {
        session: token.clone(),
        you: session.player.clone(),
        room,
        valid_commands,
    })
}

/// Build the [`RoomView`] for a room: its config and full seat roster, with each
/// occupant resolved to its public [`PlayerId`]. Decklist contents are never
/// exposed — only `decked`/`ready` flags, both `false` until issue #112.
fn build_room_view(registry: &Registry, room_id: &RoomId) -> Option<RoomView> {
    let room = registry.rooms.get(room_id)?;
    let seats = room
        .seats
        .iter()
        .enumerate()
        .map(|(index, occupant)| {
            let occupied_by = occupant
                .as_ref()
                .and_then(|tok| registry.sessions.get(tok))
                .map(|session| session.player.clone());
            SeatView {
                seat: index as u8,
                occupied_by,
                decked: false,
                ready: false,
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
fn push_view(registry: &Registry, token: &SessionToken) {
    if let Some(view) = build_view(registry, token) {
        if let Some(session) = registry.sessions.get(token) {
            let _ = session.outbox.send(Some(view));
        }
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

/// Bridge a live WebSocket connection to the lobby for its pre-game phase.
///
/// This is the pre-game analogue of [`serve_connection`](crate::serve_connection):
/// it registers a session (receiving the initial [`LobbyView`]), then pumps the
/// socket both ways until either side closes. Decoded [`LobbyCommand`]s are routed
/// through [`Lobby::command`]; every [`LobbyView`] the lobby pushes is serialized to
/// JSON and written back. On exit the session is disconnected, vacating its seat.
///
/// It carries **no game logic** — it only (de)serializes the lobby protocol and
/// routes commands to the authoritative registry. Construction of the engine game
/// and the switch to the in-game `GameView` contract happen at the ready gate
/// (issue #112), not here.
///
/// `shutdown` lets the layer-1 server stop the bridge on server shutdown: when it
/// resolves, the session is released and the socket is closed politely, just as if
/// the peer had hung up.
pub async fn serve_lobby_connection<S, F>(lobby: Lobby, ws: WebSocketStream<S>, shutdown: F)
where
    S: AsyncRead + AsyncWrite + Unpin,
    F: Future<Output = ()>,
{
    let (mut write, mut read) = ws.split();
    let (outbox_tx, mut outbox_rx) = watch::channel::<Option<LobbyView>>(None);
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
                    if let Some(view) = latest {
                        match serde_json::to_string(&view) {
                            Ok(json) => {
                                if write.send(Message::Text(json)).await.is_err() {
                                    break;
                                }
                            }
                            Err(error) => {
                                warn!(token = %handle.token, %error, "failed to serialize lobby view");
                            }
                        }
                    }
                }
                Err(_) => break,
            },
        }
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
    use rune_protocol::{Ready, SubmitDeck};

    fn lobby(max_rooms: usize) -> Lobby {
        Lobby::bundled(max_rooms).expect("bundled cards")
    }

    fn config(seats: u8) -> RoomConfig {
        RoomConfig {
            seats,
            game_setup: "standard_2p".to_string(),
        }
    }

    /// A test client: a registered session plus its outbox receiver. Holds the
    /// connection `generation` too so it can build a [`SessionHandle`] for disconnect.
    struct Client {
        token: SessionToken,
        generation: u64,
        rx: watch::Receiver<Option<LobbyView>>,
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

        /// The latest view pushed to this client (awaiting the next change).
        async fn view(&mut self) -> LobbyView {
            self.rx.changed().await.expect("a view was pushed");
            self.rx
                .borrow_and_update()
                .clone()
                .expect("pushed view is never the initial empty slot")
        }

        /// The current view without waiting for a further change.
        fn current(&self) -> LobbyView {
            self.rx.borrow().clone().expect("a view is present")
        }
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
        // Only the create/join commands are legal before a room exists.
        assert_eq!(
            view.valid_commands,
            vec!["create_room".to_string(), "join_room".to_string()]
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
        assert_eq!(view.valid_commands, vec!["leave".to_string()]);
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
        // Two seats, both filled — yet with no ready gate (issue #112) nobody starts
        // a game: both occupants stay in the lobby phase. This is what retires the
        // old "live with one player and empty decks" behavior.
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

        // Both remain in the lobby: the only interactivity is `leave`, never actions.
        assert_eq!(bob.view().await.valid_commands, vec!["leave".to_string()]);
        assert_eq!(alice.view().await.valid_commands, vec!["leave".to_string()]);
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
            vec!["create_room".to_string(), "join_room".to_string()]
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
                LobbyCommand::CreateRoom(CreateRoom { config: config(3) }),
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
    async fn leave_without_a_room_and_deferred_commands_are_typed_errors() {
        let lobby = lobby(4);
        let mut alice = Client::connect(&lobby).await;
        let _ = alice.view().await;

        assert_eq!(
            lobby.command(&alice.token, LobbyCommand::Leave).await,
            Err(LobbyError::NotInRoom)
        );
        // The deck/ready gate is issue #112: these commands are not yet handled.
        assert_eq!(
            lobby
                .command(
                    &alice.token,
                    LobbyCommand::SubmitDeck(SubmitDeck::default())
                )
                .await,
            Err(LobbyError::Unsupported)
        );
        assert_eq!(
            lobby
                .command(&alice.token, LobbyCommand::Ready(Ready { ready: true }))
                .await,
            Err(LobbyError::Unsupported)
        );
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
