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
//! # Reclaiming rooms
//! A pre-game room's registry entry — and the [`Lobby::max_rooms`] capacity it holds
//! — is freed once the room is **empty** (every seat vacated by a `Leave` or a
//! disconnect). Reclamation runs opportunistically on room creation (so freed
//! capacity is available to the next creator, even at the cap) and after every
//! disconnect/leave. Migrated from the game-over/abandonment reaping of issue #54,
//! which reaped *game-task* rooms; a pre-game room has no game task to observe, so
//! "can no longer make progress" reduces to "no occupants remain".
//!
//! # Identity is minimal for now
//! The session token is issued fresh on every connection. Reuniting a returning
//! connection with a held-open seat via an echoed token is the reconnect mechanism
//! deferred to issue #113; this module issues and tracks the token but treats every
//! `Hello` as a fresh identity.

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

/// One live connection's server-side state.
struct Session {
    /// The public player identity shown to other seats as [`SeatView::occupied_by`].
    player: PlayerId,
    /// The room this session currently occupies, if any.
    room: Option<RoomId>,
    /// The seat index within [`Session::room`], if seated.
    seat: Option<usize>,
    /// Where this connection's [`LobbyView`]s are pushed.
    outbox: LobbyOutbox,
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

    /// Register a freshly accepted connection: issue it a session token and public
    /// identity, store its `outbox`, and push it its initial [`LobbyView`] (a
    /// roomless view offering `create_room`/`join_room`). Returns the token so the
    /// connection can later address the session.
    pub(crate) async fn connect(&self, outbox: LobbyOutbox) -> SessionToken {
        let mut registry = self.inner.registry.write().await;
        let n = registry.next_session;
        registry.next_session += 1;
        let token = format!("s{n}");
        let player = format!("p{n}");
        registry.sessions.insert(
            token.clone(),
            Session {
                player,
                room: None,
                seat: None,
                outbox,
            },
        );
        push_view(&registry, &token);
        info!(%token, "connection entered the lobby");
        token
    }

    /// Retire a session when its connection ends: vacate its seat (if any), reclaim
    /// the room when it becomes empty, and notify any remaining occupants.
    ///
    /// A stale token is ignored, so a double disconnect cannot corrupt the registry.
    pub(crate) async fn disconnect(&self, token: &SessionToken) {
        let mut registry = self.inner.registry.write().await;
        let Some(session) = registry.sessions.remove(token) else {
            return;
        };
        if let (Some(room_id), Some(seat)) = (session.room, session.seat) {
            vacate(&mut registry, &room_id, seat);
            reap_empty(&mut registry);
            if registry.rooms.contains_key(&room_id) {
                push_room(&registry, &room_id);
            }
            info!(%token, %room_id, seat, "connection left the lobby; seat vacated");
        }
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
            // First contact / reconnect. Reconnect-by-token is issue #113; for now a
            // fresh identity is already issued at connect, so acknowledge by
            // re-sending the current view.
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
    // writer arm below sends it as the connection's first frame.
    let token = lobby.connect(outbox_tx).await;

    tokio::pin!(shutdown);
    loop {
        tokio::select! {
            () = &mut shutdown => break,
            incoming = read.next() => match incoming {
                Some(Ok(Message::Text(text))) => {
                    forward_lobby_command(&lobby, &token, text.as_str()).await;
                }
                Some(Ok(Message::Ping(payload))) => {
                    if write.send(Message::Pong(payload)).await.is_err() {
                        break;
                    }
                }
                Some(Ok(Message::Close(_))) | None => break,
                Some(Ok(_)) => {} // binary/pong/raw frames carry no protocol message
                Some(Err(error)) => {
                    warn!(%token, %error, "websocket read error");
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
                            Err(error) => warn!(%token, %error, "failed to serialize lobby view"),
                        }
                    }
                }
                Err(_) => break,
            },
        }
    }

    lobby.disconnect(&token).await;
    let _ = write.close().await;
}

/// Decode one JSON [`LobbyCommand`] and route it; malformed frames are logged and
/// dropped rather than closing the connection.
async fn forward_lobby_command(lobby: &Lobby, token: &SessionToken, text: &str) {
    match serde_json::from_str::<LobbyCommand>(text) {
        Ok(command) => {
            if let Err(error) = lobby.command(token, command).await {
                warn!(%token, %error, "rejected lobby command");
            }
        }
        Err(error) => warn!(%token, %error, "ignoring undecodable lobby command"),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use rune_protocol::{Hello, Ready, SubmitDeck};

    fn lobby(max_rooms: usize) -> Lobby {
        Lobby::bundled(max_rooms).expect("bundled cards")
    }

    fn config(seats: u8) -> RoomConfig {
        RoomConfig {
            seats,
            game_setup: "standard_2p".to_string(),
        }
    }

    /// A test client: a registered session plus its outbox receiver.
    struct Client {
        token: SessionToken,
        rx: watch::Receiver<Option<LobbyView>>,
    }

    impl Client {
        async fn connect(lobby: &Lobby) -> Self {
            let (tx, rx) = watch::channel(None);
            let token = lobby.connect(tx).await;
            Self { token, rx }
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
    async fn disconnect_reclaims_a_solo_room_and_notifies_remaining_peers() {
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
            .unwrap();
        let _ = bob.view().await;
        let _ = alice.view().await;

        // Bob's connection drops: alice sees his seat vacated, room still hers.
        lobby.disconnect(&bob.token).await;
        let alice_after = alice.view().await.room.expect("alice keeps the room");
        assert!(alice_after.seats[1].occupied_by.is_none());

        // Now alice drops too: the room is empty and reclaimed. A fresh joiner by the
        // old id gets an unknown-room error, proving the entry is gone.
        lobby.disconnect(&alice.token).await;
        let mut carol = Client::connect(&lobby).await;
        let _ = carol.view().await;
        let err = lobby
            .command(&carol.token, LobbyCommand::JoinRoom(JoinRoom { room_id }))
            .await
            .expect_err("the reclaimed room is gone");
        assert_eq!(err, LobbyError::UnknownRoom);
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
    async fn hello_is_acknowledged_with_a_fresh_view() {
        // Reconnect-by-token is issue #113; for now Hello just re-sends the view.
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
