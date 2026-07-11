//! End-to-end tests for the CLI **agent** session loop.
//!
//! Like the interactive session test, these drive the real layer-2 room task
//! ([`rune_server::Room`] + [`rune_server::serve_connection`]) over an in-memory
//! duplex WebSocket, but replace scripted stdin with a deterministic fake
//! [`Agent`]. That proves agent mode speaks the exact wire protocol the room
//! expects — no live model, network, or secrets — and that a chosen action drives
//! the engine.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::Duration;

use rune_cli::{run_agent_session, Agent, AgentError};
use rune_engine::{CardDatabase, GameState};
use rune_protocol::GameView;
use rune_server::{serve_connection, Room, RoomHandle, RoomInput, Seat};
use tokio::io::{duplex, DuplexStream};
use tokio::sync::watch;
use tokio_tungstenite::tungstenite::protocol::Role;
use tokio_tungstenite::WebSocketStream;

/// A deterministic fake: always chooses the offered pass-priority action. This is
/// the stub the acceptance criteria call for — the [`Agent`] trait exercised with
/// no model behind it.
struct PassingAgent;

impl Agent for PassingAgent {
    async fn choose(&self, view: &GameView) -> Result<String, AgentError> {
        view.valid_actions
            .iter()
            .find(|action| action.kind == "pass_priority")
            .map(|action| action.id.clone())
            .ok_or_else(|| AgentError::Backend("no pass offered".to_string()))
    }
}

/// Bridge one server-side socket to `seat` of `room`; named so the spawned future
/// has a concrete type.
async fn serve(seat: Seat, room: RoomHandle, ws: WebSocketStream<DuplexStream>) {
    // This bridge only ends when the peer or room does, so it never needs a
    // shutdown signal (`serve_connection` gained the parameter in issue #42).
    serve_connection(seat, room, ws, std::future::pending::<()>()).await;
}

#[tokio::test]
async fn fake_agent_completes_a_pass_priority_round() {
    let db = CardDatabase::bundled().expect("bundled cards");
    let (handle, room_task) = Room::new(GameState::new_two_player(), db).spawn();

    let (server_io, client_io) = duplex(8192);
    let server_ws = WebSocketStream::from_raw_socket(server_io, Role::Server, None).await;
    let client_ws = WebSocketStream::from_raw_socket(client_io, Role::Client, None).await;
    let bridge = tokio::spawn(serve(0, handle.clone(), server_ws));

    // Observe seat 1 directly so the test can tell when seat 0's agent pass has
    // actually been applied by the engine.
    let (tx1, mut rx1) = watch::channel::<Option<GameView>>(None);
    assert!(handle.send(RoomInput::Join {
        seat: 1,
        outbox: tx1,
    }));

    let mut log: Vec<u8> = Vec::new();

    // Run the agent until the observer confirms priority reached seat 1, then let
    // `select!` drop the (now idle-waiting) agent future. The pass frame is fully
    // sent and applied before p1 can be observed, so cancelling here is safe.
    let observe_pass = async {
        loop {
            match rx1.changed().await {
                Ok(()) => match rx1.borrow_and_update().clone() {
                    Some(view) if view.priority_player.as_deref() == Some("p1") => break true,
                    _ => continue,
                },
                Err(_) => break false,
            }
        }
    };

    let passed = tokio::select! {
        result = run_agent_session(client_ws, &PassingAgent, Duration::from_secs(5), &mut log) => {
            result.expect("agent session runs cleanly");
            // The agent should not finish before the pass is observed.
            false
        }
        reached_p1 = observe_pass => reached_p1,
    };

    assert!(passed, "the fake agent's pass moved priority to seat 1");
    let text = String::from_utf8(log).expect("utf-8 log");
    assert!(
        text.contains("chose"),
        "the agent logged its decision:\n{text}"
    );

    drop(handle);
    bridge.abort();
    let _ = bridge.await;
    room_task.abort();
    let _ = room_task.await;
}
