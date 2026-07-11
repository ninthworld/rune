//! Integration test for the layer-1 accept path: a real WebSocket client
//! connects to a server bound on an ephemeral port, the handshake succeeds, a
//! frame round-trips through the echo handler, and the server shuts down
//! gracefully when signalled.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use futures_util::{SinkExt, StreamExt};
use rune_server::{Config, Server};
use tokio::sync::oneshot;
use tokio_tungstenite::tungstenite::Message;

#[tokio::test]
async fn client_handshake_and_echo_then_graceful_shutdown() {
    // Bind to port 0 so the OS picks a free port; learn it before serving.
    let config = Config {
        addr: "127.0.0.1:0".to_string(),
    };
    let server = Server::bind(&config).await.expect("bind");
    let addr = server.local_addr();

    // Drive shutdown from the test rather than Ctrl-C.
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let handle = tokio::spawn(async move {
        server
            .run(async {
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

    // Echo: whatever we send comes back.
    ws.send(Message::Text("ping".into())).await.expect("send");
    let echoed = ws.next().await.expect("stream open").expect("message");
    assert_eq!(echoed, Message::Text("ping".into()));

    // Graceful shutdown: signal the server and confirm the task finishes.
    shutdown_tx.send(()).expect("signal shutdown");
    handle.await.expect("server task join");
}
