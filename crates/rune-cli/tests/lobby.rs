//! End-to-end test for the CLI **lobby flow** (ADR 0012, issue #115).
//!
//! Two CLI clients drive the whole pre-game protocol against the real layer-1
//! [`rune_server::Lobby`] over the in-memory duplex transport — the same in-process
//! WebSocket the server's own pre-game test uses (`crates/rune-server/tests/pregame.rs`)
//! and that the #32 CLI session harness established (`tests/session.rs`). One client
//! runs the unattended agent driver ([`run_agent_lobby_session`]) to **create** a
//! room, deck, and ready; the other runs it to **join** that room by id, deck, and
//! ready. When the last seat readies, the server constructs the game and switches
//! both sockets to the in-game `GameView` contract — which each CLI's lobby driver
//! detects and logs, and the game agent then acts on.
//!
//! This proves the CLI speaks the exact lobby wire protocol the server expects and
//! transitions cleanly from lobby to game, end to end, with no human present.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::Duration;

use rune_cli::{
    run_agent_lobby_session, run_lobby_session, Agent, AgentError, LobbyConfig, RoomAction,
};
use rune_protocol::{GameView, RoomConfig};
use rune_server::{serve_lobby_connection, Lobby};
use tokio::io::{AsyncReadExt, BufReader, DuplexStream};
use tokio_tungstenite::tungstenite::protocol::Role;
use tokio_tungstenite::WebSocketStream;

/// A 40-card decklist over the bundled ids 1..=6, as wire card identities — the same
/// shape the server's pre-game test submits.
fn decklist() -> Vec<String> {
    (0..40).map(|i| ((i % 6) + 1).to_string()).collect()
}

/// Attach a fresh CLI-facing socket to `lobby`, spawning its server-side bridge, and
/// return the client half for a CLI driver to speak over.
async fn connect(lobby: &Lobby) -> WebSocketStream<DuplexStream> {
    let (server_io, client_io) = tokio::io::duplex(16 * 1024);
    let server_ws = WebSocketStream::from_raw_socket(server_io, Role::Server, None).await;
    let client_ws = WebSocketStream::from_raw_socket(client_io, Role::Client, None).await;
    let lobby = lobby.clone();
    tokio::spawn(async move {
        serve_lobby_connection(lobby, server_ws, std::future::pending::<()>()).await;
    });
    client_ws
}

/// Read from `log` (an agent's decision stream) until `marker` appears, returning the
/// accumulated text. Used to detect the lobby→game hand-off deterministically instead
/// of racing a timer.
async fn read_until(mut log: DuplexStream, marker: &str) -> String {
    let mut acc = String::new();
    let mut buf = [0u8; 256];
    loop {
        match log.read(&mut buf).await {
            Ok(0) => return acc,
            Ok(n) => {
                acc.push_str(&String::from_utf8_lossy(&buf[..n]));
                if acc.contains(marker) {
                    return acc;
                }
            }
            Err(_) => return acc,
        }
    }
}

/// Drive one unattended CLI from `plan` until its lobby driver hands off to the game
/// (logging `MARKER`), then stop it. Returns the driver's captured log.
///
/// The game agent is the built-in pass stub. `run_agent_lobby_session` never returns
/// on its own here (the constructed game runs on), so the run future is cancelled the
/// instant the hand-off marker is observed on its log — exactly the observe-then-drop
/// pattern the #32 session harness uses.
async fn drive_to_game(ws: WebSocketStream<DuplexStream>, plan: LobbyConfig) -> String {
    // The agent's log goes down a duplex so the test can watch for the hand-off.
    let (log_reader, log_writer) = tokio::io::duplex(4096);
    let run = run_agent_lobby_session(ws, &PassStub, Duration::from_secs(5), log_writer, &plan);
    let observe = read_until(log_reader, MARKER);
    tokio::select! {
        result = run => {
            result.expect("agent lobby session runs cleanly");
            String::new()
        }
        text = observe => text,
    }
}

/// The exact marker `run_agent_lobby_session` logs the instant the ready gate passes
/// and the game view arrives.
const MARKER: &str = "game started";

/// A deterministic pass agent (the same stub the agent-session test uses).
struct PassStub;
impl Agent for PassStub {
    async fn choose(&self, view: &GameView) -> Result<String, AgentError> {
        view.valid_actions
            .iter()
            .find(|action| action.kind == "pass_priority")
            .map(|action| action.id.clone())
            .ok_or_else(|| AgentError::Backend("no pass offered".to_string()))
    }
}

#[tokio::test]
async fn two_cli_clients_drive_lobby_to_game_start() {
    let lobby = Lobby::bundled(Lobby::DEFAULT_MAX_ROOMS).expect("bundled cards");

    // Creator: opens the first room in a fresh lobby (deterministically "r0"), decks,
    // and auto-readies.
    let creator = LobbyConfig {
        action: Some(RoomAction::Create(RoomConfig {
            seats: 2,
            game_setup: "standard_2p".to_string(),
        })),
        deck: Some(decklist()),
        auto_ready: true,
    };
    // Joiner: joins that room by its known id, decks, and auto-readies.
    let joiner = LobbyConfig {
        action: Some(RoomAction::Join("r0".to_string())),
        deck: Some(decklist()),
        auto_ready: true,
    };

    let creator_ws = connect(&lobby).await;
    let joiner_ws = connect(&lobby).await;

    // Both must make progress concurrently: the gate only trips once *both* have
    // readied, so neither driver reaches the game until the other does too.
    let (creator_log, joiner_log) = tokio::join!(
        drive_to_game(creator_ws, creator),
        drive_to_game(joiner_ws, joiner),
    );

    assert!(
        creator_log.contains(MARKER),
        "the creating CLI drove the lobby to a game start:\n{creator_log}"
    );
    assert!(
        creator_log.contains("creating a 2-seat room"),
        "the creator logged its create step:\n{creator_log}"
    );
    assert!(
        joiner_log.contains(MARKER),
        "the joining CLI drove the lobby to a game start:\n{joiner_log}"
    );
    assert!(
        joiner_log.contains("joining room r0"),
        "the joiner logged its join step:\n{joiner_log}"
    );
    // Both logged submitting a deck and readying along the way.
    assert!(creator_log.contains("submitting a 40-card deck"));
    assert!(joiner_log.contains("readying up"));
}

/// Read from `reader` into `acc` until `marker` appears (or the stream ends).
async fn accumulate_until(reader: &mut DuplexStream, acc: &mut String, marker: &str) {
    let mut buf = [0u8; 256];
    while !acc.contains(marker) {
        match reader.read(&mut buf).await {
            Ok(0) => return,
            Ok(n) => acc.push_str(&String::from_utf8_lossy(&buf[..n])),
            Err(_) => return,
        }
    }
}

#[tokio::test]
async fn interactive_cli_drives_numbered_menus_to_a_game_start() {
    let lobby = Lobby::bundled(Lobby::DEFAULT_MAX_ROOMS).expect("bundled cards");

    // The creator is an unattended agent that opens "r0", decks, and readies. Because
    // it readies while the second seat is still empty, the gate does not trip: it goes
    // idle, ready, having pushed all its lobby views *before* the interactive client
    // connects — so the interactive client's view sequence stays deterministic.
    let creator_plan = LobbyConfig {
        action: Some(RoomAction::Create(RoomConfig {
            seats: 2,
            game_setup: "standard_2p".to_string(),
        })),
        deck: Some(decklist()),
        auto_ready: true,
    };
    let creator_ws = connect(&lobby).await;
    let (mut creator_log_reader, creator_log_writer) = tokio::io::duplex(4096);
    let creator_task = tokio::spawn(async move {
        let agent = PassStub;
        run_agent_lobby_session(
            creator_ws,
            &agent,
            Duration::from_secs(5),
            creator_log_writer,
            &creator_plan,
        )
        .await
        .expect("creator agent runs cleanly");
    });

    // Wait until the creator has readied (and thus finished pushing its pre-game
    // views) before the interactive client joins.
    let mut creator_log = String::new();
    accumulate_until(&mut creator_log_reader, &mut creator_log, "readying up").await;

    // The interactive client joins by the known id, submits a deck, and readies — all
    // by typing menu numbers and sub-prompt answers, exactly as an operator would.
    // Menus: roomless [create_room, join_room] → "2"; then a room id; in-room
    // [submit_deck, leave] → "1" + a decklist; in-room [submit_deck, ready, leave] →
    // "2" to ready. Once the game starts it faces the pre-game mulligan decision
    // (issue #156): the collapsed `mulligan_decision` action ("1") whose `option`
    // prompt offers keep ("1") or mulligan — so it keeps its opening hand and the
    // game proceeds past the mulligan rather than stalling the moment it is offered
    // a decision. Input then runs out and the client exits at the next prompt.
    let deck_csv = decklist().join(",");
    let scripted = format!("2\nr0\n1\n{deck_csv}\n2\n1\n1\n");
    let stdin = BufReader::new(scripted.as_bytes());
    let (mut out_reader, out_writer) = tokio::io::duplex(64 * 1024);

    // Drive the interactive session to completion while draining everything it prints
    // into `output`, so the assertions see the full transcript regardless of which
    // side of the pump finishes first (the client exits on stdin EOF partway into the
    // game, which must not race away the captured output).
    let joiner_ws = connect(&lobby).await;
    let mut output = String::new();
    {
        let run = run_lobby_session(joiner_ws, stdin, out_writer);
        tokio::pin!(run);
        let mut buf = [0u8; 1024];
        loop {
            tokio::select! {
                result = &mut run => {
                    result.expect("interactive lobby session runs cleanly");
                    break;
                }
                read = out_reader.read(&mut buf) => match read {
                    Ok(0) | Err(_) => break,
                    Ok(n) => output.push_str(&String::from_utf8_lossy(&buf[..n])),
                },
            }
        }
        // Drain anything still buffered after the session returned.
        loop {
            match out_reader.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n) => output.push_str(&String::from_utf8_lossy(&buf[..n])),
            }
        }
    }

    // The interactive client rendered lobby menus and reached the game hand-off.
    assert!(
        output.contains("LOBBY"),
        "renders the lobby display:\n{output}"
    );
    assert!(
        output.contains("Join a room by id"),
        "renders the join command as a numbered menu item:\n{output}"
    );
    assert!(
        output.contains("Game starting!"),
        "the interactive client reached the game hand-off:\n{output}"
    );

    // The creator, unblocked by the joiner readying, also reaches the game.
    accumulate_until(&mut creator_log_reader, &mut creator_log, MARKER).await;
    assert!(
        creator_log.contains(MARKER),
        "the creator reached the game once the joiner readied:\n{creator_log}"
    );

    creator_task.abort();
    let _ = creator_task.await;
}
