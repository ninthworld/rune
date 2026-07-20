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

use rune_cli::{run_agent_session, Agent, AgentError, RuleBasedAgent};
use rune_engine::{CardDatabase, CardId, FunctionalId, GameSetup, GameState};
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

/// The six bundled cards these decks are built from: three red creatures and two burn
/// spells, plus a Mountain to cast them with — every card castable off Mountains. Named
/// by authored `functional_id` (ADR 0018 §3) — a `CardId` is interned from the catalog's
/// sort order, so an integer deck would silently become a different (and, with no land in
/// it, unplayable) deck the next time a card is added.
const STARTER_CARDS: [&str; 6] = [
    "onakke_ogre",
    "fire_elemental",
    "shock",
    "lightning_strike",
    "mountain",
    "viashino_pyromancer",
];

/// A 40-card mono-red starter deck (red creatures and burn + Mountain), resolved from
/// the catalog by authored identity.
fn decklist(db: &CardDatabase) -> Vec<CardId> {
    (0..40)
        .map(|i| {
            let slug = FunctionalId::try_from(STARTER_CARDS[i % 6].to_string())
                .expect("a well-formed identity");
            db.card_id(&slug).expect("a bundled card")
        })
        .collect()
}

/// Two [`RuleBasedAgent`]s play a **full game to completion over the real socket
/// loop** ([`run_agent_session`]): the production path that reads each `GameView`,
/// asks the agent, fills its prompt/requirement slots, and echoes the token +
/// targets on the wire. The seeded room begins in the London mulligan and runs until
/// the engine declares a winner, at which point the room closes both sockets and both
/// sessions return cleanly. This proves the agent's filled answers (mulligan keep,
/// cleanup discard, combat declarations) round-trip through `resolve_action` end to
/// end — not just the direct policy path in `tests/agent_game.rs`.
#[tokio::test]
async fn two_rule_based_agents_play_a_full_game_over_the_socket_loop() {
    let db = CardDatabase::bundled().expect("bundled cards");
    let setup = GameSetup::two_player(decklist(&db), decklist(&db), 0x5EED_0000_0000_1959);
    let state = GameState::new(&setup, &db).expect("valid setup");
    let (handle, room_task) = Room::new(state, db).spawn();

    // Each seat: a duplex socket bridged into the room, driven by a rule-based agent.
    let mut sessions = Vec::new();
    let mut bridges = Vec::new();
    for seat in 0..2usize {
        let (server_io, client_io) = duplex(64 * 1024);
        let server_ws = WebSocketStream::from_raw_socket(server_io, Role::Server, None).await;
        let client_ws = WebSocketStream::from_raw_socket(client_io, Role::Client, None).await;
        bridges.push(tokio::spawn(serve(seat, handle.clone(), server_ws)));
        sessions.push(tokio::spawn(async move {
            let mut log: Vec<u8> = Vec::new();
            run_agent_session(client_ws, &RuleBasedAgent, Duration::from_secs(5), &mut log)
                .await
                .expect("agent session runs cleanly");
        }));
    }
    // Drop our handle so the room stops once it reaches a terminal state.
    drop(handle);

    // Both sessions return only when the room closes their sockets on game over. A
    // timeout guards against a stall (a policy that fails to answer some slot).
    for session in sessions {
        tokio::time::timeout(Duration::from_secs(30), session)
            .await
            .expect("the game finished and the socket closed before the timeout")
            .expect("agent task did not panic");
    }

    room_task
        .await
        .expect("the room task terminates after the game is over");
    for bridge in bridges {
        let _ = bridge.await;
    }
}
