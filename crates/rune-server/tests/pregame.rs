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

    /// Read frames until a structured lobby-error frame (issue #395) arrives,
    /// returning its `lobby_error` object. Asserts no `GameView` slips through first.
    async fn next_lobby_error(&mut self) -> serde_json::Value {
        loop {
            let value = self.next_json().await;
            assert!(
                value.get("phase").is_none(),
                "unexpected game view while awaiting a lobby error: {value}"
            );
            if let Some(error) = value.get("lobby_error") {
                return error.clone();
            }
        }
    }
}

/// A room config for a specific `game_setup` format (issue #395 rejection tests).
fn config_in(seats: u8, game_setup: &str) -> RoomConfig {
    RoomConfig {
        seats,
        game_setup: game_setup.to_string(),
    }
}

/// Open a fresh room under `game_setup` with `alice` as the seat-0 host, returning its
/// id. The caller submits (bad) decks straight through the seat.
async fn create_room_under(alice: &mut Client, game_setup: &str) -> RoomId {
    let _ = alice.lobby_view_where(|v| v.room.is_none()).await;
    alice
        .send(LobbyCommand::CreateRoom(CreateRoom {
            config: config_in(2, game_setup),
        }))
        .await;
    alice
        .lobby_view_where(|v| v.room.is_some())
        .await
        .room
        .expect("alice in room")
        .room_id
}

/// Submit `cards`/`commander` to a fresh `game_setup` room and return the structured
/// rejection the submitting seat receives (issue #395).
async fn rejection_for(
    lobby: &Lobby,
    game_setup: &str,
    cards: Vec<String>,
    commander: Option<String>,
) -> serde_json::Value {
    let mut alice = Client::connect(lobby).await;
    let _ = create_room_under(&mut alice, game_setup).await;
    alice
        .send(LobbyCommand::SubmitDeck(SubmitDeck { cards, commander }))
        .await;
    alice.next_lobby_error().await
}

#[tokio::test]
async fn issue_395_below_minimum_deck_is_rejected_with_a_reason() {
    let lobby = Lobby::bundled(Lobby::DEFAULT_MAX_ROOMS).expect("bundled cards");
    // Ten Forests: well under the starter format's 40-card minimum.
    let error = rejection_for(&lobby, "starter-1v1", vec!["forest".to_string(); 10], None).await;
    assert_eq!(error["code"], "below_minimum");
    assert_eq!(
        error["reason"],
        "deck has 10 cards, below the 40-card minimum"
    );
    // A size rejection names no specific card.
    assert!(error.get("card").is_none());
}

#[tokio::test]
async fn issue_395_copy_limit_deck_is_rejected_naming_the_card() {
    let lobby = Lobby::bundled(Lobby::DEFAULT_MAX_ROOMS).expect("bundled cards");
    // Five Onakke Ogre (over the 4-copy limit) plus 35 Forests = 40 cards.
    let mut cards = vec!["onakke_ogre".to_string(); 5];
    cards.extend(std::iter::repeat_n("forest".to_string(), 35));
    let error = rejection_for(&lobby, "starter-1v1", cards, None).await;
    assert_eq!(error["code"], "copy_limit");
    // The reason names the card by its display name, and `card` is its identity.
    assert_eq!(error["card"], "onakke_ogre");
    assert_eq!(
        error["reason"],
        "Onakke Ogre appears 5 times, above the 4-copy limit"
    );
}

#[tokio::test]
async fn issue_395_unknown_card_is_rejected_with_the_identity() {
    let lobby = Lobby::bundled(Lobby::DEFAULT_MAX_ROOMS).expect("bundled cards");
    let error = rejection_for(
        &lobby,
        "starter-1v1",
        vec!["no_such_card".to_string()],
        None,
    )
    .await;
    assert_eq!(error["code"], "unknown_card");
    assert_eq!(error["card"], "no_such_card");
    assert_eq!(error["reason"], "unknown card identity no_such_card");
}

#[tokio::test]
async fn issue_395_above_maximum_commander_deck_is_rejected() {
    let lobby = Lobby::bundled(Lobby::DEFAULT_MAX_ROOMS).expect("bundled cards");
    // 101 cards: one over the exact-100 commander size.
    let mut cards = commander_decklist();
    cards.push("forest".to_string());
    let error = rejection_for(&lobby, "commander", cards, Some("jedit_ojanen".to_string())).await;
    assert_eq!(error["code"], "above_maximum");
    assert_eq!(
        error["reason"],
        "deck has 101 cards, above the 100-card maximum"
    );
    assert!(error.get("card").is_none());
}

#[tokio::test]
async fn issue_395_missing_commander_is_rejected() {
    let lobby = Lobby::bundled(Lobby::DEFAULT_MAX_ROOMS).expect("bundled cards");
    let error = rejection_for(&lobby, "commander", commander_decklist(), None).await;
    assert_eq!(error["code"], "missing_commander");
    assert!(error.get("card").is_none());
}

#[tokio::test]
async fn issue_395_illegal_commander_is_rejected_naming_the_designation() {
    let lobby = Lobby::bundled(Lobby::DEFAULT_MAX_ROOMS).expect("bundled cards");
    // Llanowar Elves is in the deck but is not legendary, so it cannot be the commander.
    let error = rejection_for(
        &lobby,
        "commander",
        commander_decklist(),
        Some("llanowar_elves".to_string()),
    )
    .await;
    assert_eq!(error["code"], "commander_not_legendary_creature");
    assert_eq!(error["card"], "llanowar_elves");

    // A commander the deck does not contain names the designation too.
    let not_in_deck = rejection_for(
        &lobby,
        "commander",
        vec!["forest".to_string(); 100],
        Some("jedit_ojanen".to_string()),
    )
    .await;
    assert_eq!(not_in_deck["code"], "commander_not_in_deck");
    assert_eq!(not_in_deck["card"], "jedit_ojanen");
}

#[tokio::test]
async fn issue_395_out_of_identity_card_is_rejected_naming_it() {
    let lobby = Lobby::bundled(Lobby::DEFAULT_MAX_ROOMS).expect("bundled cards");
    // Swap a Forest for a blue card: outside Jedit Ojanen's green identity.
    let mut cards = commander_decklist();
    let last = cards.len() - 1;
    cards[last] = "snapping_drake".to_string();
    let error = rejection_for(&lobby, "commander", cards, Some("jedit_ojanen".to_string())).await;
    assert_eq!(error["code"], "out_of_identity");
    assert_eq!(error["card"], "snapping_drake");
}

#[tokio::test]
async fn issue_395_a_seats_rejection_reaches_no_other_seat_and_a_fix_resubmits() {
    // Redaction (issue #395): one seat's rejected deck is delivered to that seat only —
    // no other seat's frame stream carries the reason or names the offending card — and
    // the builder can correct and resubmit successfully in the same room session.
    let lobby = Lobby::bundled(Lobby::DEFAULT_MAX_ROOMS).expect("bundled cards");

    let mut alice = Client::connect(&lobby).await;
    let room_id = create_room_under(&mut alice, "starter-1v1").await;

    let mut bob = Client::connect(&lobby).await;
    let _ = bob.lobby_view_where(|v| v.room.is_none()).await;
    bob.send(LobbyCommand::JoinRoom(JoinRoom {
        room_id: room_id.clone(),
    }))
    .await;
    let _ = bob.lobby_view_where(|v| v.room.is_some()).await;

    // Alice submits an illegal deck (five Onakke Ogre + 35 Forests) and gets the reason.
    let mut bad = vec!["onakke_ogre".to_string(); 5];
    bad.extend(std::iter::repeat_n("forest".to_string(), 35));
    alice
        .send(LobbyCommand::SubmitDeck(SubmitDeck {
            cards: bad,
            commander: None,
        }))
        .await;
    let error = alice.next_lobby_error().await;
    assert_eq!(error["code"], "copy_limit");
    assert_eq!(error["card"], "onakke_ogre");

    // Alice corrects the list (four Onakke Ogre) and resubmits — accepted in the same
    // room session, her seat becomes decked.
    let mut good = vec!["onakke_ogre".to_string(); 4];
    good.extend(std::iter::repeat_n("forest".to_string(), 36));
    alice
        .send(LobbyCommand::SubmitDeck(SubmitDeck {
            cards: good,
            commander: None,
        }))
        .await;
    let _ = alice
        .lobby_view_where(|v| v.room.as_ref().is_some_and(|r| r.seats[0].decked))
        .await;

    // Bob's whole frame stream, up to seeing Alice decked from her *valid* resubmit,
    // never carried a lobby-error frame — the rejection did not leak to his seat. Had
    // it leaked, a `lobby_error` frame would arrive before the decked roster update.
    loop {
        let value = bob.next_json().await;
        assert!(
            value.get("lobby_error").is_none(),
            "another seat's rejection leaked to bob: {value}"
        );
        let view: LobbyView = serde_json::from_value(value).expect("valid LobbyView");
        if view.room.as_ref().is_some_and(|r| r.seats[0].decked) {
            break;
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

/// A legal 100-card commander decklist (issue #372): Jedit Ojanen (a green
/// legendary creature) as the commander, the catalog's in-identity green (and
/// colorless) non-basics as singletons, and Forests to fill to exactly 100 — every
/// card within Jedit's green color identity.
fn commander_decklist() -> Vec<String> {
    let non_basics = [
        "jedit_ojanen",
        "llanowar_elves",
        "druid_of_the_cowl",
        "giant_spider",
        "colossal_dreadmaw",
        "gigantosaurus",
        "titanic_growth",
        "skyscanner",
    ];
    let mut cards: Vec<String> = non_basics.iter().map(|s| s.to_string()).collect();
    while cards.len() < 100 {
        cards.push("forest".to_string());
    }
    cards
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
        .send(LobbyCommand::SubmitDeck(SubmitDeck {
            cards: decklist(),
            commander: None,
        }))
        .await;
    let alice_decked = alice
        .lobby_view_where(|v| v.room.as_ref().is_some_and(|r| r.seats[0].decked))
        .await;
    assert!(alice_decked.valid_commands.contains(&"ready".to_string()));

    bob.send(LobbyCommand::SubmitDeck(SubmitDeck {
        cards: decklist(),
        commander: None,
    }))
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

#[tokio::test]
async fn issue_372_commander_game_starts_at_forty_life_with_command_zone_visible() {
    // A commander room accepts a legal 100-card singleton in-identity deck with a
    // designated commander, starts every seat at 40 life (CR 903.7), and puts each
    // commander in a command zone every seat can see (CR 903.6), with the tax owed
    // (CR 903.8) projected publicly.
    let lobby = Lobby::bundled(Lobby::DEFAULT_MAX_ROOMS).expect("bundled cards");

    let mut alice = Client::connect(&lobby).await;
    let _ = alice.lobby_view_where(|v| v.room.is_none()).await;
    alice
        .send(LobbyCommand::CreateRoom(CreateRoom {
            config: RoomConfig {
                seats: 2,
                game_setup: "commander".to_string(),
            },
        }))
        .await;
    let room_id = alice
        .lobby_view_where(|v| v.room.is_some())
        .await
        .room
        .expect("alice in room")
        .room_id;

    let mut bob = Client::connect(&lobby).await;
    let _ = bob.lobby_view_where(|v| v.room.is_none()).await;
    bob.send(LobbyCommand::JoinRoom(JoinRoom {
        room_id: room_id.clone(),
    }))
    .await;
    let _ = bob.lobby_view_where(|v| v.room.is_some()).await;

    // Each seat submits the same legal commander deck, designating Jedit Ojanen.
    for (client, seat) in [(&mut alice, 0usize), (&mut bob, 1usize)] {
        client
            .send(LobbyCommand::SubmitDeck(SubmitDeck {
                cards: commander_decklist(),
                commander: Some("jedit_ojanen".to_string()),
            }))
            .await;
        let _ = client
            .lobby_view_where(|v| v.room.as_ref().is_some_and(|r| r.seats[seat].decked))
            .await;
    }

    alice.send(LobbyCommand::Ready(Ready { ready: true })).await;
    let _ = alice
        .lobby_view_where(|v| v.room.as_ref().is_some_and(|r| r.seats[0].ready))
        .await;
    bob.send(LobbyCommand::Ready(Ready { ready: true })).await;

    let game = alice.first_game_view().await;
    // 40 starting life (CR 903.7) for the receiver and the opponent.
    assert_eq!(game.me.life, 40);
    assert_eq!(game.opponents[0].life, 40);
    // The command zone is public: both seats' commanders are visible (CR 903.6).
    assert_eq!(game.command.len(), 2, "both command zones are public");
    for pile in &game.command {
        assert_eq!(pile.cards.len(), 1);
        assert_eq!(pile.cards[0].name, "Jedit Ojanen");
    }
    // The commander tax is public and starts at zero (no casts yet, CR 903.8).
    assert_eq!(game.commander_tax.len(), 2);
    assert!(game
        .commander_tax
        .iter()
        .all(|t| t.tax == 0 && t.casts == 0));
    // The commander is set aside, so the library is the 100-card deck minus the
    // commander minus the seven-card opening hand.
    assert_eq!(game.my_hand.len(), 7);
    assert_eq!(game.me.library_size, 100 - 1 - 7);
}
