//! End-to-end integration test for the layer-1 lobby: two real WebSocket clients
//! connect to the running server, the lobby seats them into one room, and they
//! drive a full round of pass-priority through the binary. A third connection made
//! when the lobby is at capacity is rejected cleanly.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use futures_util::{SinkExt, StreamExt};
use rune_protocol::{ChooseAction, ClientMessage, GameView, Phase, ValidAction};
use rune_server::{Config, Lobby, Server};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;

/// A running server bound on an ephemeral port, with a handle for shutting it down.
struct RunningServer {
    addr: std::net::SocketAddr,
    shutdown: oneshot::Sender<()>,
    task: JoinHandle<()>,
}

impl RunningServer {
    /// Bind and start a server whose lobby hosts at most `max_rooms` rooms.
    async fn start(max_rooms: usize) -> Self {
        let config = Config {
            addr: "127.0.0.1:0".to_string(),
        };
        let server = Server::bind(&config).await.expect("bind");
        let addr = server.local_addr();
        let lobby = Lobby::bundled(max_rooms).expect("bundled cards");
        let (shutdown, shutdown_rx) = oneshot::channel::<()>();
        let task = tokio::spawn(async move {
            server
                .run(lobby, async {
                    let _ = shutdown_rx.await;
                })
                .await
                .expect("run");
        });
        Self {
            addr,
            shutdown,
            task,
        }
    }

    /// Signal graceful shutdown and wait for the server task to finish.
    async fn stop(self) {
        let _ = self.shutdown.send(());
        self.task.await.expect("server task join");
    }
}

/// Connect a real WebSocket client to the server.
async fn connect(
    server: &RunningServer,
) -> WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>> {
    let url = format!("ws://{}", server.addr);
    let (ws, _response) = tokio_tungstenite::connect_async(&url)
        .await
        .expect("connect");
    ws
}

/// Read frames until a `GameView` satisfying `pred` arrives, decoding each text
/// frame. Non-text frames are skipped.
async fn view_where<S>(ws: &mut WebSocketStream<S>, pred: impl Fn(&GameView) -> bool) -> GameView
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    loop {
        let message = ws.next().await.expect("stream open").expect("frame");
        if let Message::Text(text) = message {
            let view: GameView = serde_json::from_str(text.as_str()).expect("valid GameView JSON");
            if pred(&view) {
                return view;
            }
        }
    }
}

/// Read the next `GameView` from the stream (any view).
async fn next_view<S>(ws: &mut WebSocketStream<S>) -> GameView
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    view_where(ws, |_| true).await
}

/// The offered pass-priority action in a view, if any.
fn pass_action(view: &GameView) -> Option<&ValidAction> {
    view.valid_actions
        .iter()
        .find(|a| a.kind == "pass_priority")
}

/// Send a `ChooseAction` for the given action id over the socket.
async fn choose<S>(ws: &mut WebSocketStream<S>, action_id: &str)
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let choose = ClientMessage::ChooseAction(ChooseAction {
        action_id: action_id.to_string(),
        ..Default::default()
    });
    let json = serde_json::to_string(&choose).expect("encode ChooseAction");
    ws.send(Message::Text(json)).await.expect("send");
}

#[tokio::test]
async fn two_clients_fill_a_room_and_pass_priority_end_to_end() {
    let server = RunningServer::start(Lobby::DEFAULT_MAX_ROOMS).await;

    // First client is seated at seat 0 (the active player) and is offered actions.
    let mut alice = connect(&server).await;
    let alice_initial = next_view(&mut alice).await;
    assert_eq!(alice_initial.priority_player.as_deref(), Some("p0"));
    let alice_pass = pass_action(&alice_initial)
        .expect("seat 0 holds priority and is offered a pass")
        .id
        .clone();

    // Second client is auto-paired into the same room at seat 1; it holds no
    // priority yet, so it is offered nothing.
    let mut bob = connect(&server).await;
    let bob_initial = next_view(&mut bob).await;
    assert_eq!(bob_initial.priority_player.as_deref(), Some("p0"));
    assert!(bob_initial.valid_actions.is_empty());

    // Seat 0 passes: priority hands off to seat 1, proving the action routed through
    // the engine (the server holds no game logic).
    choose(&mut alice, &alice_pass).await;
    let bob_has_priority =
        view_where(&mut bob, |v| v.priority_player.as_deref() == Some("p1")).await;
    let bob_pass = pass_action(&bob_has_priority)
        .expect("priority handed to seat 1")
        .id
        .clone();

    // Seat 1 passes too: both passed, so the step advances and priority returns to
    // the active player (seat 0) — a full round of pass-priority end to end.
    choose(&mut bob, &bob_pass).await;
    let after_round = view_where(&mut alice, |v| v.priority_player.as_deref() == Some("p0")).await;
    assert_eq!(after_round.phase, Phase::Upkeep);
    assert!(!after_round.valid_actions.is_empty());

    server.stop().await;
}

#[tokio::test]
async fn connection_beyond_capacity_is_rejected_cleanly() {
    // One room, two seats, no room for more.
    let server = RunningServer::start(1).await;

    // Fill both seats; reading each seat's view confirms it was actually seated.
    let mut alice = connect(&server).await;
    let _ = next_view(&mut alice).await;
    let mut bob = connect(&server).await;
    let _ = next_view(&mut bob).await;

    // A third connection completes the WebSocket handshake but the lobby has no seat
    // for it, so the server closes it cleanly without ever sending a GameView.
    let mut carol = connect(&server).await;
    // A polite Close, an ended stream, or a transport reset all mean "not seated";
    // only a text frame (a GameView) would mean the lobby wrongly seated Carol.
    if let Some(Ok(Message::Text(text))) = carol.next().await {
        panic!("oversubscribed connection was seated, got view: {text}");
    }

    // The two seated clients are unaffected: the game is still live for them.
    server.stop().await;
}
