//! Integration test for the pre-game gate (ADR 0012, issue #112) over the
//! in-memory duplex transport: two WebSocket peers drive the lobby end to end —
//! create a room, join by id, submit decks, and ready up — and, when the last seat
//! readies, the server constructs the game from the submitted decks and hands both
//! connections off to the in-game `GameView` contract on the *same socket*.
//!
//! It exercises the whole hand-off `serve_lobby_connection` performs, not just the
//! registry: the frames a real client receives switch from `LobbyView` JSON to
//! `GameView` JSON exactly at the gate, and the first `GameView` reflects the
//! shuffled decks (7-card opening hand, 33-card library).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use futures_util::{SinkExt, StreamExt};
use rune_protocol::{
    CreateRoom, GameView, JoinRoom, LobbyCommand, LobbyView, Ready, RoomConfig, RoomId, SubmitDeck,
};
use rune_server::{serve_lobby_connection, Lobby};
use tokio::io::DuplexStream;
use tokio_tungstenite::tungstenite::protocol::Role;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;

/// A connected test client: the client half of a duplex WebSocket whose server half
/// is being served through the shared lobby.
struct Client {
    ws: WebSocketStream<DuplexStream>,
}

impl Client {
    /// Attach a fresh connection to `lobby`, spawning its server-side bridge.
    async fn connect(lobby: &Lobby) -> Self {
        let (server_io, client_io) = tokio::io::duplex(16 * 1024);
        let server_ws = WebSocketStream::from_raw_socket(server_io, Role::Server, None).await;
        let client_ws = WebSocketStream::from_raw_socket(client_io, Role::Client, None).await;
        let lobby = lobby.clone();
        tokio::spawn(async move {
            serve_lobby_connection(lobby, server_ws, std::future::pending::<()>()).await;
        });
        Self { ws: client_ws }
    }

    /// Send one lobby command.
    async fn send(&mut self, command: LobbyCommand) {
        let json = serde_json::to_string(&command).expect("encode LobbyCommand");
        self.ws.send(Message::Text(json)).await.expect("send");
    }

    /// Read frames until a `LobbyView` satisfying `pred` arrives.
    async fn lobby_view_where(&mut self, pred: impl Fn(&LobbyView) -> bool) -> LobbyView {
        loop {
            let value = self.next_json().await;
            // Before the gate every frame is a `LobbyView`; a `GameView` (which
            // carries `phase`) would mean the game started unexpectedly.
            assert!(
                value.get("phase").is_none(),
                "unexpected game view before the ready gate: {value}"
            );
            let view: LobbyView = serde_json::from_value(value).expect("valid LobbyView");
            if pred(&view) {
                return view;
            }
        }
    }

    /// Read frames until the first `GameView` (the post-gate hand-off) arrives,
    /// skipping any remaining pre-game `LobbyView`s.
    async fn first_game_view(&mut self) -> GameView {
        loop {
            let value = self.next_json().await;
            if value.get("phase").is_some() {
                return serde_json::from_value(value).expect("valid GameView");
            }
        }
    }

    /// Decode the next text frame as a JSON value.
    async fn next_json(&mut self) -> serde_json::Value {
        loop {
            let message = self.ws.next().await.expect("stream open").expect("frame");
            if let Message::Text(text) = message {
                return serde_json::from_str(&text).expect("valid JSON frame");
            }
        }
    }
}

fn config(seats: u8) -> RoomConfig {
    RoomConfig {
        seats,
        game_setup: "standard_2p".to_string(),
    }
}

/// The six bundled cards these decks are built from: five green creatures and a Forest
/// to cast them with. A decklist names cards by authored `functional_id` (ADR 0018 §3),
/// never by `CardId` — that handle is interned from the catalog's sort order, so an
/// integer deck would come to mean different cards as soon as one is added.
const STARTER_CARDS: [&str; 6] = [
    "onakke_ogre",
    "snapping_drake",
    "fire_elemental",
    "giant_spider",
    "forest",
    "walking_corpse",
];

/// A legal 40-card decklist as the wire carries it.
fn decklist() -> Vec<String> {
    (0..40).map(|i| STARTER_CARDS[i % 6].to_string()).collect()
}

async fn create_two_seat_room(alice: &mut Client) -> RoomId {
    // Alice lands roomless, creates a two-seat room, and learns its shareable id.
    let _ = alice.lobby_view_where(|v| v.room.is_none()).await;
    alice
        .send(LobbyCommand::CreateRoom(CreateRoom { config: config(2) }))
        .await;
    alice
        .lobby_view_where(|v| v.room.is_some())
        .await
        .room
        .expect("alice in room")
        .room_id
}

#[tokio::test]
async fn deck_submit_and_ready_gate_constructs_the_game_and_hands_off_both_seats() {
    let lobby = Lobby::bundled(Lobby::DEFAULT_MAX_ROOMS).expect("bundled cards");

    // Create → join.
    let mut alice = Client::connect(&lobby).await;
    let room_id = create_two_seat_room(&mut alice).await;

    let mut bob = Client::connect(&lobby).await;
    let _ = bob.lobby_view_where(|v| v.room.is_none()).await;
    bob.send(LobbyCommand::JoinRoom(JoinRoom {
        room_id: room_id.clone(),
    }))
    .await;
    let _ = bob.lobby_view_where(|v| v.room.is_some()).await;

    // Submit decks: each seat becomes decked and is offered `ready`.
    alice
        .send(LobbyCommand::SubmitDeck(SubmitDeck { cards: decklist() }))
        .await;
    let alice_decked = alice
        .lobby_view_where(|v| v.room.as_ref().is_some_and(|r| r.seats[0].decked))
        .await;
    assert!(alice_decked.valid_commands.contains(&"ready".to_string()));

    bob.send(LobbyCommand::SubmitDeck(SubmitDeck { cards: decklist() }))
        .await;
    let _ = bob
        .lobby_view_where(|v| v.room.as_ref().is_some_and(|r| r.seats[1].decked))
        .await;

    // Ready ×2. The last ready trips the gate; nothing game-related was sent before.
    alice.send(LobbyCommand::Ready(Ready { ready: true })).await;
    let _ = alice
        .lobby_view_where(|v| v.room.as_ref().is_some_and(|r| r.seats[0].ready))
        .await;
    bob.send(LobbyCommand::Ready(Ready { ready: true })).await;

    // Both connections are handed off to the game: the first GameView reflects the
    // shuffled decks — a seven-card opening hand and a 33-card library.
    let alice_game = alice.first_game_view().await;
    assert_eq!(alice_game.you, "p0");
    assert_eq!(alice_game.my_hand.len(), 7);
    assert_eq!(alice_game.opponents.len(), 1);
    assert_eq!(alice_game.opponents[0].hand_size, 7);
    assert_eq!(alice_game.opponents[0].library_size, 40 - 7);

    let bob_game = bob.first_game_view().await;
    assert_eq!(bob_game.you, "p1");
    assert_eq!(bob_game.my_hand.len(), 7);
    assert_eq!(bob_game.opponents[0].library_size, 40 - 7);
}
