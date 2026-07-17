//! Integration test for the layer-1 accept path with the lobby wired in: a real
//! WebSocket client connects to a server bound on an ephemeral port, the handshake
//! succeeds, the lobby lands it in the pre-game phase with a `LobbyView` (a session
//! token and the create/join commands — never a `GameView`), and the server shuts
//! down gracefully when signalled.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use futures_util::StreamExt;
use rune_protocol::LobbyView;
use rune_server::{Config, Lobby, Server};
use tokio::sync::oneshot;
use tokio_tungstenite::tungstenite::Message;

#[tokio::test]
async fn client_lands_in_the_lobby_then_server_shuts_down_gracefully() {
    // Bind to port 0 so the OS picks a free port; learn it before serving.
    let config = Config {
        addr: "127.0.0.1:0".to_string(),
        rng_seed: None,
        starting_life: None,
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

    // The lobby immediately pushes the pre-game `LobbyView`: a session token, no
    // room yet, and the create/join commands. Crucially, it is *not* a `GameView` —
    // no engine game is constructed until the ready gate (issue #112).
    let text = loop {
        match ws.next().await.expect("stream open").expect("message") {
            Message::Text(text) => break text,
            _ => continue,
        }
    };
    let view: LobbyView = serde_json::from_str(text.as_str()).expect("valid LobbyView JSON");
    assert!(!view.session.is_empty(), "a session token was issued");
    assert!(view.room.is_none(), "a fresh connection is in no room");
    assert_eq!(
        view.valid_commands,
        vec![
            "set_name".to_string(),
            "create_room".to_string(),
            "join_room".to_string()
        ]
    );

    // Graceful shutdown: signal the server and confirm the task finishes.
    shutdown_tx.send(()).expect("signal shutdown");
    handle.await.expect("server task join");
}
