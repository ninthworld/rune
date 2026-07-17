//! End-to-end integration test for the layer-1 lobby: real WebSocket clients
//! connect to the running server and drive the explicit-room protocol — create a
//! room with a config, share its id, join by id, and reconnect to a held seat by
//! session token — over the wire (ADR 0012, issues #110 and #113). No game is
//! constructed: the connections stay in the pre-game phase, exchanging
//! `LobbyView`/`LobbyCommand`, until the ready gate (issue #112).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use futures_util::{SinkExt, StreamExt};
use rune_protocol::{
    CreateRoom, Hello, JoinRoom, LobbyCommand, LobbyView, Ready, RoomConfig, RoomState, SubmitDeck,
};
use rune_server::{Config, Lobby, Server};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;

/// A running server bound on an ephemeral port, with a handle for shutting it down.
struct RunningServer {
    addr: std::net::SocketAddr,
    shutdown: oneshot::Sender<()>,
    task: JoinHandle<()>,
}

impl RunningServer {
    /// Bind and start a server whose lobby hosts at most `max_rooms` rooms.
    async fn start(max_rooms: usize) -> Self {
        let config = Config {
            addr: "127.0.0.1:0".to_string(),
            rng_seed: None,
            starting_life: None,
        };
        let server = Server::bind(&config).await.expect("bind");
        let addr = server.local_addr();
        let lobby = Lobby::bundled(max_rooms).expect("bundled cards");
        let (shutdown, shutdown_rx) = oneshot::channel::<()>();
        let task = tokio::spawn(async move {
            server
                .run(lobby, async {
                    let _ = shutdown_rx.await;
                })
                .await
                .expect("run");
        });
        Self {
            addr,
            shutdown,
            task,
        }
    }

    /// Signal graceful shutdown and wait for the server task to finish.
    async fn stop(self) {
        let _ = self.shutdown.send(());
        self.task.await.expect("server task join");
    }
}

/// Connect a real WebSocket client to the server.
async fn connect(
    server: &RunningServer,
) -> WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>> {
    let url = format!("ws://{}", server.addr);
    let (ws, _response) = tokio_tungstenite::connect_async(&url)
        .await
        .expect("connect");
    ws
}

/// Read frames until a `LobbyView` satisfying `pred` arrives, decoding each text
/// frame. Non-text frames are skipped.
async fn view_where<S>(ws: &mut WebSocketStream<S>, pred: impl Fn(&LobbyView) -> bool) -> LobbyView
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    loop {
        let message = ws.next().await.expect("stream open").expect("frame");
        if let Message::Text(text) = message {
            let view: LobbyView =
                serde_json::from_str(text.as_str()).expect("valid LobbyView JSON");
            if pred(&view) {
                return view;
            }
        }
    }
}

/// Read the next `LobbyView` from the stream (any view).
async fn next_view<S>(ws: &mut WebSocketStream<S>) -> LobbyView
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    view_where(ws, |_| true).await
}

/// Send a `LobbyCommand` over the socket.
async fn send<S>(ws: &mut WebSocketStream<S>, command: LobbyCommand)
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let json = serde_json::to_string(&command).expect("encode LobbyCommand");
    ws.send(Message::Text(json)).await.expect("send");
}

fn config(seats: u8) -> RoomConfig {
    RoomConfig {
        seats,
        game_setup: "standard_2p".to_string(),
    }
}

/// A legal 40-card decklist expressed as wire card identities (four copies each of
/// the five non-basics, plus twenty basic Forests) — enough for the ready gate to
/// construct a real game so the directory can flip a room to `in_progress`.
fn decklist() -> Vec<String> {
    let mut cards = Vec::new();
    for slug in [
        "thornback_boar",
        "riverbank_otter",
        "emberfang_jackal",
        "stonehide_basilisk",
        "verdant_scout",
    ] {
        for _ in 0..4 {
            cards.push(slug.to_string());
        }
    }
    for _ in 0..20 {
        cards.push("forest".to_string());
    }
    cards
}

#[tokio::test]
async fn two_clients_create_and_join_a_room_by_id_end_to_end() {
    let server = RunningServer::start(Lobby::DEFAULT_MAX_ROOMS).await;

    // Alice lands in the lobby (roomless) and creates a two-seat room.
    let mut alice = connect(&server).await;
    let alice_initial = next_view(&mut alice).await;
    assert!(alice_initial.room.is_none());
    send(
        &mut alice,
        LobbyCommand::CreateRoom(CreateRoom { config: config(2) }),
    )
    .await;

    // Her next view carries the freshly issued, shareable room id and seats her at 0.
    let alice_room = view_where(&mut alice, |v| v.room.is_some()).await;
    let room = alice_room.room.expect("alice is in a room");
    let room_id = room.room_id.clone();
    assert!(!room_id.is_empty());
    assert_eq!(room.config.seats, 2);
    assert_eq!(
        room.seats[0].occupied_by.as_deref(),
        Some(alice_room.you.as_str())
    );
    assert!(room.seats[1].occupied_by.is_none());

    // Bob lands in the lobby and joins by the shared id.
    let mut bob = connect(&server).await;
    let _ = next_view(&mut bob).await;
    send(
        &mut bob,
        LobbyCommand::JoinRoom(JoinRoom {
            room_id: room_id.clone(),
        }),
    )
    .await;

    // Bob is seated at seat 1 of the same room — no game starts, only a full roster.
    let bob_room = view_where(&mut bob, |v| v.room.is_some()).await;
    let bob_view_room = bob_room.room.expect("bob is in a room");
    assert_eq!(bob_view_room.room_id, room_id);
    assert_eq!(
        bob_view_room.seats[1].occupied_by.as_deref(),
        Some(bob_room.you.as_str())
    );
    // Seated but undecked: bob may submit a deck or leave (the ready gate, #112).
    assert_eq!(
        bob_room.valid_commands,
        vec![
            "set_name".to_string(),
            "submit_deck".to_string(),
            "leave".to_string()
        ]
    );

    // Alice is pushed an updated roster showing both seats filled.
    let alice_full = view_where(&mut alice, |v| {
        v.room
            .as_ref()
            .is_some_and(|r| r.seats.iter().all(|s| s.occupied_by.is_some()))
    })
    .await;
    assert!(alice_full.room.is_some());

    server.stop().await;
}

#[tokio::test]
async fn joining_an_unknown_room_leaves_the_client_in_the_lobby() {
    let server = RunningServer::start(Lobby::DEFAULT_MAX_ROOMS).await;

    // A client that joins a nonexistent room is rejected: the current LobbyView is
    // re-sent (still roomless) rather than seating it anywhere.
    let mut carol = connect(&server).await;
    let initial = next_view(&mut carol).await;
    assert!(initial.room.is_none());
    send(
        &mut carol,
        LobbyCommand::JoinRoom(JoinRoom {
            room_id: "r-nope".to_string(),
        }),
    )
    .await;
    let after = next_view(&mut carol).await;
    assert!(
        after.room.is_none(),
        "unknown-room join never seats the client"
    );

    server.stop().await;
}

#[tokio::test]
async fn create_beyond_capacity_is_refused_but_joining_still_works() {
    // Capacity for exactly one room.
    let server = RunningServer::start(1).await;

    // Alice creates the one allowed room.
    let mut alice = connect(&server).await;
    let _ = next_view(&mut alice).await;
    send(
        &mut alice,
        LobbyCommand::CreateRoom(CreateRoom { config: config(2) }),
    )
    .await;
    let room_id = view_where(&mut alice, |v| v.room.is_some())
        .await
        .room
        .expect("alice in room")
        .room_id;

    // Bob cannot create another room (at capacity): his view stays roomless. He can
    // still join Alice's room by id — capacity limits room creation, not joining.
    let mut bob = connect(&server).await;
    let _ = next_view(&mut bob).await;
    send(
        &mut bob,
        LobbyCommand::CreateRoom(CreateRoom { config: config(2) }),
    )
    .await;
    let refused = next_view(&mut bob).await;
    assert!(refused.room.is_none(), "create at capacity is refused");

    send(&mut bob, LobbyCommand::JoinRoom(JoinRoom { room_id })).await;
    let joined = view_where(&mut bob, |v| v.room.is_some()).await;
    assert!(
        joined.room.is_some(),
        "joining an existing room still works at capacity"
    );

    server.stop().await;
}

#[tokio::test]
async fn a_returning_socket_reconnects_to_its_held_seat_by_token_end_to_end() {
    // Full-stack reconnect (issue #113): a client creates a room, its socket drops,
    // and a brand-new socket presenting the stored session token is routed back into
    // the same seat and resynced from one `LobbyView`.
    let server = RunningServer::start(Lobby::DEFAULT_MAX_ROOMS).await;

    // Alice creates a room and records her secret session token and public identity.
    let mut alice = connect(&server).await;
    let _ = next_view(&mut alice).await;
    send(
        &mut alice,
        LobbyCommand::CreateRoom(CreateRoom { config: config(2) }),
    )
    .await;
    let seated = view_where(&mut alice, |v| v.room.is_some()).await;
    let room_id = seated.room.expect("alice in room").room_id;
    let token = seated.session.clone();
    let you = seated.you.clone();
    assert!(!token.is_empty(), "a session token was issued");

    // Her socket drops. The server holds the seat open for the token to return.
    drop(alice);

    // A fresh socket presents the token via `hello` and is reunited with the seat.
    let mut returning = connect(&server).await;
    let _ = next_view(&mut returning).await; // the fresh identity issued at connect
    send(
        &mut returning,
        LobbyCommand::Hello(Hello {
            token: Some(token.clone()),
        }),
    )
    .await;

    // One `LobbyView` fully resyncs the returning connection into the same room and
    // seat, under the same public identity — the reconstruct-from-one-view invariant.
    let resumed = view_where(&mut returning, |v| v.room.is_some()).await;
    let room = resumed.room.expect("reconnected into the held room");
    assert_eq!(room.room_id, room_id, "same room, not a new one");
    assert_eq!(resumed.session, token, "same session token echoed back");
    assert_eq!(resumed.you, you, "same public identity across reconnect");
    assert_eq!(
        room.seats[0].occupied_by.as_deref(),
        Some(you.as_str()),
        "routed back into the original seat 0",
    );

    server.stop().await;
}

#[tokio::test]
async fn a_browser_discovers_a_room_joins_it_and_sees_it_start_end_to_end() {
    // Issue #280: the room directory lets a second connection discover an open game
    // without an out-of-band id — see it appear, its occupancy change, and it flip to
    // `in_progress` at game start — and join straight from the listed id (zero copy
    // paste). Carol is a roomless *browser* that watches the directory throughout.
    let server = RunningServer::start(Lobby::DEFAULT_MAX_ROOMS).await;

    // Carol connects first and browses: no rooms exist yet.
    let mut carol = connect(&server).await;
    let carol_initial = next_view(&mut carol).await;
    assert!(carol_initial.room.is_none());
    assert!(carol_initial.directory.is_empty(), "no rooms to browse yet");

    // Alice creates a two-seat room.
    let mut alice = connect(&server).await;
    let _ = next_view(&mut alice).await;
    send(
        &mut alice,
        LobbyCommand::CreateRoom(CreateRoom { config: config(2) }),
    )
    .await;
    let room_id = view_where(&mut alice, |v| v.room.is_some())
        .await
        .room
        .expect("alice in room")
        .room_id;

    // Carol sees the room appear in her directory with NO action on her part:
    // one of two seats filled, gathering, and joinable by the listed id.
    let carol_sees = view_where(&mut carol, |v| !v.directory.is_empty()).await;
    let entry = carol_sees
        .directory
        .iter()
        .find(|r| r.room_id == room_id)
        .expect("the created room is listed");
    assert_eq!(entry.config.seats, 2);
    assert_eq!(entry.filled, 1);
    assert_eq!(entry.state, RoomState::Gathering);

    // Bob joins straight from the directory id — the same id Carol can see, no
    // out-of-band sharing.
    let mut bob = connect(&server).await;
    let _ = next_view(&mut bob).await;
    send(
        &mut bob,
        LobbyCommand::JoinRoom(JoinRoom {
            room_id: room_id.clone(),
        }),
    )
    .await;
    let _ = view_where(&mut bob, |v| v.room.is_some()).await;

    // Carol sees the occupancy tick to 2/2, still gathering.
    let carol_full = view_where(&mut carol, |v| {
        v.directory
            .iter()
            .any(|r| r.room_id == room_id && r.filled == 2)
    })
    .await;
    let entry = carol_full
        .directory
        .iter()
        .find(|r| r.room_id == room_id)
        .expect("still listed");
    assert_eq!(entry.state, RoomState::Gathering);

    // Both seats submit a deck and ready up, passing the gate and constructing a game.
    send(
        &mut alice,
        LobbyCommand::SubmitDeck(SubmitDeck { cards: decklist() }),
    )
    .await;
    let _ = view_where(&mut alice, |v| {
        v.room.as_ref().is_some_and(|r| r.seats[0].decked)
    })
    .await;
    send(&mut alice, LobbyCommand::Ready(Ready { ready: true })).await;
    let _ = view_where(&mut alice, |v| {
        v.room.as_ref().is_some_and(|r| r.seats[0].ready)
    })
    .await;

    send(
        &mut bob,
        LobbyCommand::SubmitDeck(SubmitDeck { cards: decklist() }),
    )
    .await;
    let _ = view_where(&mut bob, |v| {
        v.room.as_ref().is_some_and(|r| r.seats[1].decked)
    })
    .await;
    // Bob readies last: the gate passes and the room's game starts.
    send(&mut bob, LobbyCommand::Ready(Ready { ready: true })).await;

    // Carol — never in the room — sees it flip to `in_progress` in her directory,
    // visible but no longer joinable.
    let carol_started = view_where(&mut carol, |v| {
        v.directory
            .iter()
            .any(|r| r.room_id == room_id && r.state == RoomState::InProgress)
    })
    .await;
    assert!(
        carol_started
            .directory
            .iter()
            .any(|r| r.room_id == room_id && r.state == RoomState::InProgress),
        "the started room shows as in_progress to a browser",
    );

    server.stop().await;
}
