//! Integration test for the layer-2 room bridge: a real WebSocket peer joins a
//! room through [`rune_server::serve_connection`], receives its personalized
//! `GameView` as JSON, drives the engine by echoing back an offered `action_id`,
//! and sees the resulting view — end to end, with no game logic on the client.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use futures_util::{SinkExt, StreamExt};
use rune_engine::{CardDatabase, GameState};
use rune_protocol::{ChooseAction, ClientMessage, GameView};
use rune_server::Room;
use tokio_tungstenite::tungstenite::protocol::Role;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;

/// Read the next text frame from the peer and decode it as a [`GameView`].
async fn next_view<S>(ws: &mut WebSocketStream<S>) -> GameView
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    loop {
        let message = ws.next().await.expect("stream open").expect("frame");
        if let Message::Text(text) = message {
            return serde_json::from_str(text.as_str()).expect("valid GameView JSON");
        }
    }
}

#[tokio::test]
async fn websocket_peer_joins_room_and_drives_pass_priority() {
    let db = CardDatabase::bundled().expect("bundled cards");
    let (handle, room_task) = Room::new(GameState::new_two_player(), db).spawn();

    // A duplex pipe stands in for a TCP connection; both ends are WebSocket
    // streams so real framing is exercised without an HTTP handshake.
    let (server_io, client_io) = tokio::io::duplex(8192);
    let server_ws = WebSocketStream::from_raw_socket(server_io, Role::Server, None).await;
    let mut client_ws = WebSocketStream::from_raw_socket(client_io, Role::Client, None).await;

    // Bridge the server side of the socket to seat 0 of the room.
    let bridge = tokio::spawn(serve(handle.clone(), server_ws));

    // On join the client is brought current with a full personalized view.
    let initial = next_view(&mut client_ws).await;
    assert_eq!(initial.priority_player.as_deref(), Some("p0"));
    let pass = initial
        .valid_actions
        .iter()
        .find(|a| a.kind == "pass_priority")
        .expect("priority holder is offered a pass");

    // Echo the offered action id back over the wire; the room applies it.
    let choose = ClientMessage::ChooseAction(ChooseAction {
        action_id: pass.id.clone(),
    });
    client_ws
        .send(Message::Text(serde_json::to_string(&choose).unwrap()))
        .await
        .expect("send choose_action");

    // Seat 0 passed: priority moved to the (unconnected) seat 1, so seat 0's next
    // view offers it nothing — proving the action routed through the engine.
    let after = next_view(&mut client_ws).await;
    assert_eq!(after.priority_player.as_deref(), Some("p1"));
    assert!(after.valid_actions.is_empty());

    // Closing the client ends the bridge; dropping the handle stops the room.
    client_ws.close(None).await.expect("close");
    bridge.await.unwrap();
    drop(handle);
    room_task.await.unwrap();
}

/// Bridge helper: named so the spawned future has a concrete type. The bridge only
/// ends when the peer or room does, so it never needs a shutdown signal here.
async fn serve(handle: rune_server::RoomHandle, ws: WebSocketStream<tokio::io::DuplexStream>) {
    rune_server::serve_connection(0, handle, ws, std::future::pending::<()>()).await;
}
