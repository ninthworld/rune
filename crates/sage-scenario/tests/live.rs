//! The whole path, over a real socket: file → position → room → engine, and back.
//!
//! This is the headless half of the proof issue #777 asks for. It drives the runner exactly
//! as the browser does — a WebSocket to the loopback address it printed, a `GameView` in,
//! a `choose_action` out — and checks that the **authoritative** position moved, not that a
//! fixture was re-rendered. The browser half lives in `clients/web/e2e/scenario.spec.ts` and
//! proves the same path through the shipping UI; this one proves it without needing one, so
//! `make check` catches a broken runner on its own.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use sage_engine::CardDatabase;
use sage_protocol::{ChooseAction, ClientMessage, GameView};
use sage_scenario::{accept, build, parse, start, Options, Running};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

/// The example scenario, which is also the one the docs walk through.
fn scenario_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/murder-the-dreadmaw.toml")
}

/// Start the example scenario on an ephemeral loopback port, with no client.
async fn running() -> Running {
    let text = std::fs::read_to_string(scenario_path()).expect("the example scenario is readable");
    let scenario = parse(&text).expect("the example scenario parses");
    let db = CardDatabase::bundled().expect("the bundled catalog loads");
    let position = build(&scenario, &db).expect("the example scenario builds");
    start(
        &position,
        db,
        &Options {
            addr: "127.0.0.1:0".to_string(),
            client_addr: "127.0.0.1:0".to_string(),
            client_dir: None,
        },
    )
    .await
    .expect("the scenario serves")
}

/// Open a browser-equivalent connection to a running scenario.
async fn browser(url: &str) -> WebSocketStream<MaybeTlsStream<TcpStream>> {
    let (socket, _) = tokio_tungstenite::connect_async(url)
        .await
        .expect("the scenario socket accepts a connection");
    socket
}

/// The permanents the receiving seat controls, which is what a scenario's board is about.
fn mine(view: &GameView) -> Vec<&sage_protocol::Permanent> {
    view.battlefield
        .iter()
        .filter(|perm| perm.controller == view.you)
        .collect()
}

/// The next `GameView` the server pushes, or a failure rather than a hang.
async fn next_view(socket: &mut WebSocketStream<MaybeTlsStream<TcpStream>>) -> GameView {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let frame = tokio::time::timeout_at(deadline, socket.next())
            .await
            .expect("a view arrives before the deadline")
            .expect("the socket is open")
            .expect("the frame reads");
        if let Message::Text(text) = frame {
            if let Ok(view) = serde_json::from_str::<GameView>(&text) {
                return view;
            }
        }
    }
}

#[tokio::test]
async fn a_click_through_the_socket_changes_the_authoritative_position() {
    let running = running().await;
    let url = format!("ws://{}", running.addr());
    tokio::spawn(accept(running, std::future::pending::<()>()));

    let mut socket = browser(&url).await;

    // 1. The position the file described arrives with no lobby, no deck, and no mulligan —
    //    the first thing this connection is ever sent is the board it asked for.
    let view = next_view(&mut socket).await;
    assert_eq!(view.turn, 6, "the authored turn");
    assert_eq!(view.you, "p0");
    assert_eq!(mine(&view).len(), 4, "three Swamps and the Elves");
    assert_eq!(view.my_hand.len(), 2, "Murder and a Swamp");
    assert_eq!(view.opponents.len(), 1);
    assert!(
        view.opponents[0].ai,
        "the AI seat is marked as one for the client"
    );
    assert!(
        !view.valid_actions.is_empty(),
        "the engine offers this seat something to do"
    );

    // 2. Take one — a land drop, which needs no cost and no target, so what is being proved
    //    is the round trip rather than the client's ability to fill a slot.
    let land = view
        .valid_actions
        .iter()
        .find(|action| action.kind == "play_land")
        .expect("a land in hand at a main phase is playable");
    socket
        .send(Message::Text(
            serde_json::to_string(&ClientMessage::ChooseAction(ChooseAction {
                action_id: land.id.clone(),
                token: land.token.clone(),
                ..Default::default()
            }))
            .expect("the message serializes"),
        ))
        .await
        .expect("the socket accepts the action");

    // 3. The engine applied it: the authoritative board has one more permanent and the hand
    //    one fewer card. Nothing about this could come from a fixture.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    let after = loop {
        let view = next_view(&mut socket).await;
        if mine(&view).len() > 4 {
            break view;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the position never changed",
        );
    };
    assert_eq!(after.my_hand.len(), 1, "the land left the hand");
    assert_eq!(
        mine(&after)
            .iter()
            .filter(|perm| perm.card.name == "Swamp")
            .count(),
        4,
        "a fourth Swamp is on the battlefield",
    );
}

#[tokio::test]
async fn a_refresh_reconstructs_the_same_live_game_from_the_server() {
    let running = running().await;
    let url = format!("ws://{}", running.addr());
    tokio::spawn(accept(running, std::future::pending::<()>()));

    let mut first = browser(&url).await;
    let before = next_view(&mut first).await;
    let land = before
        .valid_actions
        .iter()
        .find(|action| action.kind == "play_land")
        .expect("a land is playable")
        .clone();
    first
        .send(Message::Text(
            serde_json::to_string(&ClientMessage::ChooseAction(ChooseAction {
                action_id: land.id.clone(),
                token: land.token.clone(),
                ..Default::default()
            }))
            .expect("the message serializes"),
        ))
        .await
        .expect("the socket accepts the action");
    loop {
        if mine(&next_view(&mut first).await).len() > 4 {
            break;
        }
    }

    // The browser goes away, exactly as a reload makes it: the seat is held open and the
    // game keeps running on the server.
    first.close(None).await.expect("the socket closes");
    drop(first);

    // A fresh connection is handed the game as it *now* stands, in one full-state view —
    // the land is still down, and no state had to survive in the page to get it back.
    let mut second = browser(&url).await;
    let after = next_view(&mut second).await;
    assert_eq!(after.turn, 6);
    assert_eq!(
        mine(&after).len(),
        5,
        "the game that was running is the game that comes back",
    );
    assert_eq!(after.my_hand.len(), 1);
}

#[tokio::test]
async fn the_same_file_and_seed_serve_the_same_opening_view() {
    // Determinism, end to end: two runs of one file are the same game. Views are compared
    // rather than states because the view is what a person actually sees differ.
    let mut views = Vec::new();
    for _ in 0..2 {
        let running = running().await;
        let url = format!("ws://{}", running.addr());
        tokio::spawn(accept(running, std::future::pending::<()>()));
        let mut socket = browser(&url).await;
        views.push(next_view(&mut socket).await);
    }
    assert_eq!(
        serde_json::to_string(&views[0]).expect("serializes"),
        serde_json::to_string(&views[1]).expect("serializes"),
    );
}
