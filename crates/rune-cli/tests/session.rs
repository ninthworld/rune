//! End-to-end tests for the CLI session loop.
//!
//! These drive the real layer-2 room task ([`rune_server::Room`] +
//! [`rune_server::serve_connection`]) over an in-memory duplex WebSocket — the
//! same in-process transport the server's own room test uses — while feeding the
//! CLI a scripted stdin and capturing its output. That proves the CLI speaks the
//! exact wire protocol the room expects: it renders the personalized `GameView`
//! frames the room pushes and its `choose_action` replies drive the engine.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use rune_cli::run_session;
use rune_engine::{CardDatabase, GameState};
use rune_protocol::GameView;
use rune_server::{serve_connection, Room, RoomHandle, RoomInput, Seat};
use tokio::io::{duplex, BufReader, DuplexStream};
use tokio::sync::watch;
use tokio_tungstenite::tungstenite::protocol::Role;
use tokio_tungstenite::WebSocketStream;

/// Bridge one server-side socket to `seat` of `room`; named so the spawned future
/// has a concrete type.
async fn serve(seat: Seat, room: RoomHandle, ws: WebSocketStream<DuplexStream>) {
    // This bridge only ends when the peer or room does, so it never needs a
    // shutdown signal (`serve_connection` gained the parameter in issue #42).
    serve_connection(seat, room, ws, std::future::pending::<()>()).await;
}

/// A connected CLI at seat 0 over the real room bridge, plus the two WebSocket
/// halves. The room starts from a fresh two-player game (priority to seat 0 at
/// the untap step, whose only offered action is a pass).
async fn setup() -> (
    RoomHandle,
    tokio::task::JoinHandle<()>,
    WebSocketStream<DuplexStream>,
) {
    let db = CardDatabase::bundled().expect("bundled cards");
    let (handle, _room_task) = Room::new(GameState::new_two_player(), db).spawn();
    let (server_io, client_io) = duplex(8192);
    let server_ws = WebSocketStream::from_raw_socket(server_io, Role::Server, None).await;
    let client_ws = WebSocketStream::from_raw_socket(client_io, Role::Client, None).await;
    let bridge = tokio::spawn(serve(0, handle.clone(), server_ws));
    (handle, bridge, client_ws)
}

#[tokio::test]
async fn renders_numbered_menu_reprompts_invalid_and_exits_on_eof() {
    let (handle, bridge, client_ws) = setup().await;

    // Two invalid entries then end-of-input: the loop must re-prompt locally and
    // then exit cleanly on EOF without ever sending an action.
    let stdin: &[u8] = b"0\nbanana\n";
    let reader = BufReader::new(stdin);
    let mut out: Vec<u8> = Vec::new();

    run_session(client_ws, reader, &mut out)
        .await
        .expect("session exits cleanly on EOF");

    let text = String::from_utf8(out).expect("utf-8 output");
    // The whole display is reconstructed from the single pushed view.
    assert!(text.contains("Priority: p0"), "renders the pushed view");
    assert!(
        text.contains("1) Pass priority"),
        "offers valid_actions as a numbered menu:\n{text}"
    );
    // Both "0" (out of range) and "banana" (non-numeric) are re-prompted locally.
    assert_eq!(
        text.matches("is not a listed choice").count(),
        2,
        "each invalid entry re-prompts:\n{text}"
    );
    assert!(text.contains("End of input"), "EOF exits cleanly");

    drop(handle);
    bridge.abort();
    let _ = bridge.await;
}

#[tokio::test]
async fn chosen_action_routes_through_the_engine_and_passes_priority() {
    let db = CardDatabase::bundled().expect("bundled cards");
    let (handle, room_task) = Room::new(GameState::new_two_player(), db).spawn();

    let (server_io, client_io) = duplex(8192);
    let server_ws = WebSocketStream::from_raw_socket(server_io, Role::Server, None).await;
    let client_ws = WebSocketStream::from_raw_socket(client_io, Role::Client, None).await;
    let bridge = tokio::spawn(serve(0, handle.clone(), server_ws));

    // Observe seat 1 directly so the test can tell when seat 0's pass — typed
    // into the CLI — has actually been applied by the engine.
    let (tx1, mut rx1) = watch::channel::<Option<GameView>>(None);
    assert!(handle.send(RoomInput::Join {
        seat: 1,
        outbox: tx1,
    }));

    // Seat 0's only action is a pass; menu entry "1" selects it.
    let stdin: &[u8] = b"1\n";
    let reader = BufReader::new(stdin);
    let mut out: Vec<u8> = Vec::new();

    // Run the CLI until the observer confirms priority reached seat 1, then let
    // `select!` drop the (now idle-waiting) CLI future. The pass frame is fully
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
        result = run_session(client_ws, reader, &mut out) => {
            result.expect("session runs cleanly");
            // The CLI should not finish before the pass is observed.
            false
        }
        reached_p1 = observe_pass => reached_p1,
    };

    assert!(passed, "seat 0's CLI pass moved priority to seat 1");
    let text = String::from_utf8(out).expect("utf-8 output");
    assert!(
        text.contains("1) Pass priority"),
        "the pass was offered and chosen by menu number:\n{text}"
    );

    drop(handle);
    bridge.abort();
    let _ = bridge.await;
    room_task.abort();
    let _ = room_task.await;
}
