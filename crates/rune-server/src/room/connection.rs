//! WebSocket bridges between a live connection and a running room: the seated
//! [`serve_connection`] and the read-only [`serve_spectator_connection`], plus the
//! message-decoding glue. These carry **no game logic** — they only (de)serialize
//! the protocol and pump the socket. Pure code motion out of the room module root
//! (issue #427) — no behavior change.

use std::future::Future;

use futures_util::{SinkExt, StreamExt};
use rune_protocol::ClientMessage;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;
use tracing::warn;

use super::*;

/// Bridge a live WebSocket connection to a room for the given `seat`.
///
/// This is the glue between the layer-1 accept path (issue #30) and a room: it
/// pumps the socket both ways until either side closes. Decoded [`ClientMessage`]s
/// flow to the room as [`RoomInput::Message`]; every [`GameView`] the room pushes
/// is serialized to JSON and written back. It joins on entry and sends
/// [`RoomInput::Leave`] on exit, so the seat is held open for a later reconnect.
///
/// It carries **no game logic** — it only (de)serializes the protocol; which
/// connection maps to which room and seat is a lobby/matchmaking concern handled
/// elsewhere.
///
/// **Slow consumer:** the room→connection path is a latest-value [`watch`], so a
/// client that cannot keep up with the write side never accumulates a backlog; it
/// simply skips superseded views and always ends up writing the newest state (see
/// the writer arm below). Neither channel this task holds can be grown without
/// bound by a slow or flooding peer (issue #57).
///
/// `shutdown` lets the layer-1 lobby stop the bridge on server shutdown: when it
/// resolves, the seat is released and the socket is closed politely, just as if the
/// peer had hung up. Pass [`std::future::pending`] for a bridge that only ever ends
/// when the peer or room does.
pub async fn serve_connection<S, F>(
    seat: Seat,
    room: RoomHandle,
    ws: WebSocketStream<S>,
    shutdown: F,
) where
    S: AsyncRead + AsyncWrite + Unpin,
    F: Future<Output = ()>,
{
    let (mut write, mut read) = ws.split();
    let (outbox_tx, mut outbox_rx) = watch::channel::<Option<GameView>>(None);
    if !room.send(RoomInput::Join {
        seat,
        outbox: outbox_tx,
    }) {
        warn!(seat, "room unavailable at join; closing connection");
        let _ = write.close().await;
        return;
    }

    tokio::pin!(shutdown);
    loop {
        tokio::select! {
            () = &mut shutdown => {
                // Server is shutting down: leave the loop and close politely below.
                break;
            }
            incoming = read.next() => match incoming {
                Some(Ok(Message::Text(text))) => {
                    forward_client_message(seat, &room, text.as_str());
                }
                Some(Ok(Message::Ping(payload))) => {
                    if write.send(Message::Pong(payload)).await.is_err() {
                        break;
                    }
                }
                Some(Ok(Message::Close(_))) | None => break,
                Some(Ok(_)) => {} // binary/pong/raw frames carry no protocol message
                Some(Err(error)) => {
                    warn!(seat, %error, "websocket read error");
                    break;
                }
            },
            // Slow-consumer story: the outbox is a latest-value `watch`. While this
            // arm is parked on `write.send(...).await` for a slow client, the room
            // may overwrite the pending view any number of times; when we loop back,
            // `changed()` fires once and we serialize only the newest snapshot. The
            // superseded intermediates are simply never sent — safe because each
            // `GameView` is a complete snapshot (`docs/protocol.md`). The channel
            // never grows, so a slow reader cannot pressure server memory.
            changed = outbox_rx.changed() => match changed {
                Ok(()) => {
                    let latest = outbox_rx.borrow_and_update().clone();
                    if let Some(view) = latest {
                        match serde_json::to_string(&view) {
                            Ok(json) => {
                                if write.send(Message::Text(json)).await.is_err() {
                                    break;
                                }
                            }
                            Err(error) => warn!(seat, %error, "failed to serialize game view"),
                        }
                    }
                }
                // The room dropped our outbox (task stopped): nothing more to send.
                Err(_) => break,
            },
        }
    }

    let _ = room.send(RoomInput::Leave { seat });
    let _ = write.close().await;
}

/// Bridge a live WebSocket connection to a room as a **spectator** (ADR 0022, issue
/// #351): a non-seated observer that receives redacted [`SpectatorView`]s and sends
/// **nothing** back. It is the read-only counterpart of [`serve_connection`] — it joins
/// via [`RoomInput::JoinSpectator`], serializes each pushed `SpectatorView` to JSON and
/// writes it, and still drains the read half so it notices a client close or answers a
/// ping, but it never decodes or forwards a `ClientMessage` (a spectator has no seat and
/// no `valid_actions`, so any frame it sends is ignored). A spectator owns no seat, so
/// there is nothing to hold open on exit — it simply drops its outbox and the room
/// prunes it on the next broadcast.
pub async fn serve_spectator_connection<S, F>(room: RoomHandle, ws: WebSocketStream<S>, shutdown: F)
where
    S: AsyncRead + AsyncWrite + Unpin,
    F: Future<Output = ()>,
{
    let (mut write, mut read) = ws.split();
    let (outbox_tx, mut outbox_rx) = watch::channel::<Option<SpectatorView>>(None);
    if !room.send(RoomInput::JoinSpectator { outbox: outbox_tx }) {
        warn!("room unavailable at spectator join; closing connection");
        let _ = write.close().await;
        return;
    }

    tokio::pin!(shutdown);
    loop {
        tokio::select! {
            () = &mut shutdown => break,
            incoming = read.next() => match incoming {
                // A spectator carries no interactivity: text frames are ignored, not
                // decoded into game actions. We still answer pings and notice a close.
                Some(Ok(Message::Ping(payload))) => {
                    if write.send(Message::Pong(payload)).await.is_err() {
                        break;
                    }
                }
                Some(Ok(Message::Close(_))) | None => break,
                Some(Ok(_)) => {} // text/binary/pong — a spectator sends nothing actionable
                Some(Err(error)) => {
                    warn!(%error, "spectator websocket read error");
                    break;
                }
            },
            changed = outbox_rx.changed() => match changed {
                Ok(()) => {
                    let latest = outbox_rx.borrow_and_update().clone();
                    if let Some(view) = latest {
                        match serde_json::to_string(&view) {
                            Ok(json) => {
                                if write.send(Message::Text(json)).await.is_err() {
                                    break;
                                }
                            }
                            Err(error) => warn!(%error, "failed to serialize spectator view"),
                        }
                    }
                }
                Err(_) => break, // the room stopped: nothing more to send
            },
        }
    }

    let _ = write.close().await;
}

/// Decode one JSON client message and forward it to the room; malformed frames are
/// logged and dropped rather than closing the connection.
fn forward_client_message(seat: Seat, room: &RoomHandle, text: &str) {
    match serde_json::from_str::<ClientMessage>(text) {
        Ok(message) => {
            let _ = room.send(RoomInput::Message { seat, message });
        }
        Err(error) => warn!(seat, %error, "ignoring undecodable client message"),
    }
}
