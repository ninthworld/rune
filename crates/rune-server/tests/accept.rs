//! Integration test for the layer-1 accept path once the lobby is wired in: a real
//! WebSocket client connects to a server bound on an ephemeral port, the handshake
//! succeeds, the lobby seats it and the room brings it current with its personalized
//! `GameView`, and the server shuts down gracefully when signalled.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use futures_util::StreamExt;
use rune_protocol::GameView;
use rune_server::{Config, Lobby, Server};
use tokio::sync::oneshot;
use tokio_tungstenite::tungstenite::Message;

#[tokio::test]
async fn client_is_seated_then_server_shuts_down_gracefully() {
    // Bind to port 0 so the OS picks a free port; learn it before serving.
    let config = Config {
        addr: "127.0.0.1:0".to_string(),
    };
    let server = Server::bind(&config).await.expect("bind");
    let addr = server.local_addr();
    let lobby = Lobby::bundled(Lobby::DEFAULT_MAX_ROOMS).expect("bundled cards");

    // Drive shutdown from the test rather than Ctrl-C.
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let handle = tokio::spawn(async move {
        server
            .run(lobby, async {
                let _ = shutdown_rx.await;
            })
            .await
            .expect("run");
    });

    // Handshake: connect a real WebSocket client.
    let url = format!("ws://{addr}");
    let (mut ws, _response) = tokio_tungstenite::connect_async(&url)
        .await
        .expect("connect");

    // Seated: the room immediately pushes this seat its personalized view. The very
    // first connection takes seat 0 (the active player), so it is offered actions.
    let text = loop {
        match ws.next().await.expect("stream open").expect("message") {
            Message::Text(text) => break text,
            _ => continue,
        }
    };
    let view: GameView = serde_json::from_str(text.as_str()).expect("valid GameView JSON");
    assert_eq!(view.priority_player.as_deref(), Some("p0"));
    assert!(!view.valid_actions.is_empty());

    // Graceful shutdown: signal the server and confirm the task finishes.
    shutdown_tx.send(()).expect("signal shutdown");
    handle.await.expect("server task join");
}
