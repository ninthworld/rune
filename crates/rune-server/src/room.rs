//! The room task — layer 2 of `docs/brief.md`.
//!
//! A room is one async task that owns exactly one [`rune_engine`] game and is the
//! sole writer of its state. Connections never touch the game directly; they send
//! [`RoomInput`] messages over the room's channel, and the room applies chosen
//! actions through the engine and pushes each connected seat its own personalized
//! [`GameView`]. Because the only mutable state lives inside the task, no two rooms
//! ever share game state.
//!
//! The room contains **no game logic**: it routes an `action_id` back to the
//! engine's own [`valid_actions`](rune_engine::valid_actions)/[`apply_action`](rune_engine::apply_action)
//! and rejects anything the engine did not offer (see [`crate::view::resolve_action`]).
//!
//! # Bounded channels and backpressure (issue #57)
//! Neither of the room's channels can be grown without bound by a slow or flooding
//! peer:
//! - **Per-seat outbox** is a [`watch`] channel that holds only the *latest*
//!   [`GameView`]. Every view is a complete snapshot that supersedes the previous
//!   one (`docs/protocol.md`), so a slow reader that falls behind simply skips the
//!   intermediate views and receives the newest state once it catches up —
//!   correctness is unaffected by the dropped intermediates. Buffer depth is
//!   structurally one.
//! - **Room inbox** is a bounded [`mpsc`] channel ([`ROOM_INBOX_CAPACITY`]). Inputs
//!   are delivered with `try_send`; once the queue is full, further inputs from a
//!   flooding client are **dropped** (logged) rather than buffered. The room stays
//!   alive and keeps serving; a client can only ever hurt its own throughput, never
//!   the server's memory.

use std::future::Future;

use futures_util::{SinkExt, StreamExt};
use rune_engine::{apply_action, CardDatabase, GameState, PlayerId};
use rune_protocol::{ClientMessage, GameView};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;
use tracing::{info, warn};

use crate::view::{personalized_view, resolve_action};

/// Bound on the room's input queue. Inputs beyond this depth from a flooding client
/// are dropped (see [`RoomHandle::send`]); the value is generous enough that a
/// well-behaved pair of clients never approaches it, yet fixed so a misbehaving peer
/// cannot grow server memory.
const ROOM_INBOX_CAPACITY: usize = 1024;

/// A seat at a room's table, identified by its engine seat index. Seat `0` is the
/// starting player. The lobby (layer 1) decides which connection occupies which
/// seat; the room trusts the seat each [`RoomInput`] carries.
pub type Seat = usize;

/// An input delivered to a running [`Room`].
///
/// Every game mutation originates from one of these, delivered over the room's
/// single input channel — that is what makes the room the sole writer of its state
/// with no cross-room sharing.
#[derive(Debug)]
pub enum RoomInput {
    /// A connection took, or reconnected to, `seat`. `outbox` is where the room
    /// pushes that seat's personalized [`GameView`]s. The room immediately sends
    /// the current view so a joining or reconnecting client is brought fully
    /// current with a single full-state message (no diff replay).
    Join {
        /// The seat this connection occupies.
        seat: Seat,
        /// Latest-value channel the room pushes this seat's views to. It is a
        /// [`watch`] so a slow reader always observes the newest [`GameView`] and
        /// never accumulates a backlog of superseded snapshots.
        outbox: watch::Sender<Option<GameView>>,
    },
    /// A connected seat sent a protocol message.
    Message {
        /// The seat the message came from.
        seat: Seat,
        /// The decoded client message.
        message: ClientMessage,
    },
    /// A connection for `seat` dropped. The seat is held open (see [`Room`]); the
    /// game state is untouched so the player can reconnect and resume.
    Leave {
        /// The seat whose connection dropped.
        seat: Seat,
    },
}

/// A cloneable handle for delivering [`RoomInput`]s to a running [`Room`] task.
#[derive(Clone, Debug)]
pub struct RoomHandle {
    inbox: mpsc::Sender<RoomInput>,
}

impl RoomHandle {
    /// Deliver an input to the room. Returns `false` only if the room task has
    /// already stopped (its receiver was dropped), so callers can give up cleanly.
    ///
    /// The inbox is bounded ([`ROOM_INBOX_CAPACITY`]); delivery is non-blocking. If
    /// the queue is momentarily full — a client flooding faster than the room can
    /// apply actions — the input is **dropped** and a warning is logged, but the
    /// room is still alive so this returns `true`. Dropping is safe: the client only
    /// starves its own progress, and every reply is a full-state snapshot, so a
    /// dropped action is simply one the client can re-send after its next view.
    pub fn send(&self, input: RoomInput) -> bool {
        match self.inbox.try_send(input) {
            Ok(()) => true,
            Err(mpsc::error::TrySendError::Full(_)) => {
                warn!("room inbox full; dropping input from a slow or flooding client");
                true
            }
            Err(mpsc::error::TrySendError::Closed(_)) => false,
        }
    }
}

/// One game room: a single-writer async task around one [`GameState`].
///
/// The room owns exactly one engine game and one [`CardDatabase`] and is the only
/// code that writes that state. Connections interact solely by sending
/// [`RoomInput`] over the channel from [`Room::spawn`].
///
/// # Disconnect / reconnect policy
/// A seat is **held open** across disconnects. A [`RoomInput::Leave`] clears only
/// that seat's outbox; the game state is never mutated. Nothing the engine offers
/// the absent player can advance without them, because the room is the sole writer
/// and only a player's own `ChooseAction` moves their decisions forward — so the
/// game effectively pauses on whoever must act next. On reconnect the client sends
/// a fresh [`RoomInput::Join`] and the room re-sends that seat's latest
/// [`GameView`] in full, honoring the full-state invariant (`docs/protocol.md`).
/// There is no per-seat timeout here; turn clocks are a later milestone.
pub struct Room {
    state: GameState,
    db: CardDatabase,
    /// Per-seat outbox, indexed by [`Seat`]. `None` means the seat is currently
    /// disconnected (held open). Each present sender is the [`watch`] half of a
    /// latest-value channel, so pushing a view never blocks the room nor buffers
    /// superseded snapshots.
    seats: Vec<Option<watch::Sender<Option<GameView>>>>,
}

impl Room {
    /// Create a room around an initial `state` and card `db`. The number of seats
    /// is fixed by `state.players`; each seat starts disconnected.
    #[must_use]
    pub fn new(state: GameState, db: CardDatabase) -> Self {
        let seats = state.players.iter().map(|_| None).collect();
        Self { state, db, seats }
    }

    /// Spawn the room on a Tokio task, returning a [`RoomHandle`] for delivering
    /// inputs and the [`JoinHandle`] of the task. The task runs until every sender
    /// half of its input channel has been dropped.
    pub fn spawn(self) -> (RoomHandle, JoinHandle<()>) {
        let (inbox_tx, inbox_rx) = mpsc::channel(ROOM_INBOX_CAPACITY);
        let handle = tokio::spawn(self.run(inbox_rx));
        (RoomHandle { inbox: inbox_tx }, handle)
    }

    /// The room's message loop. Exposed for tests and embedders that want to drive
    /// a room on their own task; most callers use [`Room::spawn`].
    ///
    /// The loop ends — and the task returns — when either the input channel closes
    /// (every [`RoomHandle`] dropped) or the game reaches a terminal state (a player
    /// has lost). On game over it pushes one final broadcast so every connected seat
    /// sees the finished board, then stops; the lobby reclaims the room afterward
    /// (issue #54).
    pub async fn run(mut self, mut inbox: mpsc::Receiver<RoomInput>) {
        while !self.game_over() {
            let Some(input) = inbox.recv().await else {
                info!("room input channel closed; room task stopping");
                return;
            };
            match input {
                RoomInput::Join { seat, outbox } => self.on_join(seat, outbox),
                RoomInput::Message { seat, message } => self.on_message(seat, &message),
                RoomInput::Leave { seat } => self.on_leave(seat),
            }
        }
        // A player has lost: the game is over. Push the terminal state to every
        // connected seat as the final broadcast, then stop — nothing further can
        // happen, and the lobby will reclaim the room (issue #54).
        self.broadcast();
        info!("game reached a terminal state; room task stopping after final broadcast");
    }

    /// Whether the game has reached a terminal state (CR 104.2a).
    ///
    /// Delegates entirely to the engine's [`GameState::is_over`], which decides
    /// terminality from the losing conditions it models; the room only reads that
    /// verdict and never decides a loss itself, keeping all game logic in the
    /// engine. On a terminal state the room stops advancing and keeps broadcasting
    /// the final view (see [`Room::run`]).
    fn game_over(&self) -> bool {
        self.state.is_over()
    }

    /// Seat (or re-seat) a connection and bring it current with a full view.
    fn on_join(&mut self, seat: Seat, outbox: watch::Sender<Option<GameView>>) {
        let Some(slot) = self.seats.get_mut(seat) else {
            warn!(seat, "join for a seat that does not exist; ignoring");
            return;
        };
        *slot = Some(outbox);
        self.send_view(seat);
    }

    /// Hold a disconnected seat open without disturbing the game.
    fn on_leave(&mut self, seat: Seat) {
        if let Some(slot) = self.seats.get_mut(seat) {
            *slot = None;
            info!(seat, "seat disconnected; held open for reconnect");
        }
    }

    /// Route a client message. A chosen action the engine offered this seat is
    /// applied and every connected seat is re-broadcast its view; anything else is
    /// rejected and the sender is simply re-sent its current view (full-state
    /// resync), never mutating the game.
    fn on_message(&mut self, seat: Seat, message: &ClientMessage) {
        match message {
            ClientMessage::ChooseAction(choose) => {
                match resolve_action(&self.state, &self.db, PlayerId(seat), choose) {
                    Some(action) => {
                        self.state = apply_action(&self.state, &action, &self.db);
                        // A terminal result is delivered once by the run loop's final
                        // broadcast; don't re-send the same full-state view here.
                        if !self.game_over() {
                            self.broadcast();
                        }
                    }
                    None => {
                        warn!(
                            seat,
                            action_id = %choose.action_id,
                            "rejected action id not offered to this seat"
                        );
                        self.send_view(seat);
                    }
                }
            }
        }
    }

    /// Push the seat's freshly-personalized view to its outbox. Writing to the
    /// latest-value [`watch`] never blocks and overwrites any view the reader has
    /// not yet consumed (coalescing to newest). If the receiver is gone, treat it as
    /// a disconnect and hold the seat open.
    fn send_view(&mut self, seat: Seat) {
        let view = personalized_view(&self.state, &self.db, PlayerId(seat));
        let Some(slot) = self.seats.get_mut(seat) else {
            return;
        };
        let Some(outbox) = slot.as_ref() else {
            return;
        };
        if outbox.send(Some(view)).is_err() {
            *slot = None;
        }
    }

    /// Send every connected seat its own personalized view.
    fn broadcast(&mut self) {
        for seat in 0..self.seats.len() {
            let connected = self.seats.get(seat).map(Option::is_some).unwrap_or(false);
            if connected {
                self.send_view(seat);
            }
        }
    }
}

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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use rune_engine::{CardId, GameState, Step};
    use rune_protocol::ChooseAction;

    fn db() -> CardDatabase {
        CardDatabase::bundled().unwrap()
    }

    /// A fresh per-seat outbox pair mirroring what a connection hands the room.
    fn view_channel() -> (
        watch::Sender<Option<GameView>>,
        watch::Receiver<Option<GameView>>,
    ) {
        watch::channel(None)
    }

    /// Receive the next (latest) view, awaiting the room task rather than
    /// busy-polling. Marks the value seen so a later [`watch::Receiver::has_changed`]
    /// reflects only views pushed after this call.
    async fn wait_for_view(rx: &mut watch::Receiver<Option<GameView>>) -> GameView {
        rx.changed().await.expect("room should push a view");
        rx.borrow_and_update()
            .clone()
            .expect("pushed view is never the initial empty slot")
    }

    /// A two-player game in the precombat main phase where player 0 holds a Forest
    /// and a Verdant Scout and player 1 holds a single card. Enough to exercise
    /// hidden-zone redaction and a real (non-pass) action.
    fn dealt_state() -> GameState {
        let mut state = GameState::new_two_player();
        state.step = Step::PrecombatMain;
        let p0_hand = vec![state.new_instance(CardId(5)), state.new_instance(CardId(6))];
        let p0_lib = vec![state.new_instance(CardId(1))];
        let p1_hand = vec![state.new_instance(CardId(1))];
        let p1_lib = vec![state.new_instance(CardId(1)), state.new_instance(CardId(1))];
        state.players[0].hand = p0_hand;
        state.players[0].library = p0_lib;
        state.players[1].hand = p1_hand;
        state.players[1].library = p1_lib;
        state
    }

    /// A two-player game whose player 1 sits at 0 life. `apply_action` always runs
    /// state-based actions, so the next applied action (even a pass) marks player 1
    /// as having lost — driving the room to a terminal state.
    fn near_terminal_state() -> GameState {
        let mut state = GameState::new_two_player();
        state.players[1].life = 0;
        state
    }

    #[tokio::test]
    async fn join_sends_each_seat_a_personalized_view_hiding_opponents_hands() {
        let (handle, task) = Room::new(dealt_state(), db()).spawn();
        let (tx0, mut rx0) = view_channel();
        let (tx1, mut rx1) = view_channel();
        assert!(handle.send(RoomInput::Join {
            seat: 0,
            outbox: tx0
        }));
        assert!(handle.send(RoomInput::Join {
            seat: 1,
            outbox: tx1
        }));

        // Give the room task a chance to process both joins.
        let view0 = wait_for_view(&mut rx0).await;
        let view1 = wait_for_view(&mut rx1).await;

        // Each seat's view names its own receiver in `you`.
        assert_eq!(view0.you, "p0");
        assert_eq!(view1.you, "p1");

        // Player 0 sees their own two cards but only a count for player 1's hand.
        assert_eq!(view0.my_hand.len(), 2);
        assert_eq!(view0.opponents.len(), 1);
        assert_eq!(view0.opponents[0].hand_size, 1);
        // The opponent view carries no card contents at all.
        assert_eq!(view0.opponents[0].library_size, 2);

        // Player 1 symmetrically sees only their own single card.
        assert_eq!(view1.my_hand.len(), 1);
        assert_eq!(view1.opponents[0].hand_size, 2);
        assert_eq!(view1.opponents[0].library_size, 1);

        // Only the priority holder (seat 0) is offered actions.
        assert!(!view0.valid_actions.is_empty());
        assert!(view1.valid_actions.is_empty());

        drop(handle);
        task.await.unwrap();
    }

    #[tokio::test]
    async fn two_players_advance_a_round_of_pass_priority() {
        let (handle, task) = Room::new(GameState::new_two_player(), db()).spawn();
        let (tx0, mut rx0) = view_channel();
        let (tx1, mut rx1) = view_channel();
        handle.send(RoomInput::Join {
            seat: 0,
            outbox: tx0,
        });
        handle.send(RoomInput::Join {
            seat: 1,
            outbox: tx1,
        });
        let initial0 = wait_for_view(&mut rx0).await;
        let _ = wait_for_view(&mut rx1).await;

        // Seat 0 holds priority: choose its "pass" action by the offered id.
        let pass0 = initial0
            .valid_actions
            .iter()
            .find(|a| a.kind == "pass_priority")
            .expect("pass offered to priority holder");
        handle.send(RoomInput::Message {
            seat: 0,
            message: ClientMessage::ChooseAction(ChooseAction {
                action_id: pass0.id.clone(),
                ..Default::default()
            }),
        });

        // After seat 0 passes, priority moves to seat 1, who is now offered a pass.
        let after0_seat1 = wait_for_view(&mut rx1).await;
        let pass1 = after0_seat1
            .valid_actions
            .iter()
            .find(|a| a.kind == "pass_priority")
            .expect("priority handed to seat 1");
        assert_eq!(after0_seat1.priority_player.as_deref(), Some("p1"));
        handle.send(RoomInput::Message {
            seat: 1,
            message: ClientMessage::ChooseAction(ChooseAction {
                action_id: pass1.id.clone(),
                ..Default::default()
            }),
        });

        // Both passed: the step advances and priority returns to the active player.
        // Seat 0 was broadcast a view after each pass; drain to the end-of-round
        // one (priority back to p0).
        let mut after_round = wait_for_view(&mut rx0).await;
        while after_round.priority_player.as_deref() != Some("p0") {
            after_round = wait_for_view(&mut rx0).await;
        }
        assert_eq!(after_round.phase, rune_protocol::Phase::Upkeep);

        drop(handle);
        task.await.unwrap();
    }

    #[tokio::test]
    async fn unknown_action_id_is_rejected_and_state_is_resent_unchanged() {
        let (handle, task) = Room::new(GameState::new_two_player(), db()).spawn();
        let (tx0, mut rx0) = view_channel();
        handle.send(RoomInput::Join {
            seat: 0,
            outbox: tx0,
        });
        let before = wait_for_view(&mut rx0).await;

        // A nonsense id is not among the offered actions: rejected.
        handle.send(RoomInput::Message {
            seat: 0,
            message: ClientMessage::ChooseAction(ChooseAction {
                action_id: "does-not-exist".to_string(),
                ..Default::default()
            }),
        });
        let resent = wait_for_view(&mut rx0).await;
        // The rejection re-sends the identical view — the game did not advance.
        assert_eq!(resent.phase, before.phase);
        assert_eq!(resent.priority_player, before.priority_player);
        assert_eq!(resent.valid_actions, before.valid_actions);

        drop(handle);
        task.await.unwrap();
    }

    #[tokio::test]
    async fn action_from_a_seat_without_priority_is_rejected() {
        let (handle, task) = Room::new(GameState::new_two_player(), db()).spawn();
        let (tx0, mut rx0) = view_channel();
        let (tx1, mut rx1) = view_channel();
        handle.send(RoomInput::Join {
            seat: 0,
            outbox: tx0,
        });
        handle.send(RoomInput::Join {
            seat: 1,
            outbox: tx1,
        });
        let _ = wait_for_view(&mut rx0).await;
        let _ = wait_for_view(&mut rx1).await;

        // Seat 1 does not hold priority; even "a0" (a real id for seat 0) is not an
        // action offered to seat 1, so it is rejected and seat 1 is resynced.
        handle.send(RoomInput::Message {
            seat: 1,
            message: ClientMessage::ChooseAction(ChooseAction {
                action_id: "a0".to_string(),
                ..Default::default()
            }),
        });
        let resent = wait_for_view(&mut rx1).await;
        assert!(resent.valid_actions.is_empty());
        // Seat 0 was never re-broadcast because nothing changed: its latest-value
        // outbox holds no view newer than the one already observed.
        assert!(!rx0.has_changed().unwrap());

        drop(handle);
        task.await.unwrap();
    }

    #[tokio::test]
    async fn reconnect_is_brought_current_with_a_full_view() {
        let (handle, task) = Room::new(dealt_state(), db()).spawn();
        let (tx0, mut rx0) = view_channel();
        handle.send(RoomInput::Join {
            seat: 0,
            outbox: tx0,
        });
        let _ = wait_for_view(&mut rx0).await;

        // Disconnect: the seat is held open, the game is untouched.
        handle.send(RoomInput::Leave { seat: 0 });

        // Reconnect with a fresh outbox: the room re-sends the latest full view.
        let (tx0b, mut rx0b) = view_channel();
        handle.send(RoomInput::Join {
            seat: 0,
            outbox: tx0b,
        });
        let resumed = wait_for_view(&mut rx0b).await;
        assert_eq!(resumed.my_hand.len(), 2);
        assert!(!resumed.valid_actions.is_empty());

        drop(handle);
        task.await.unwrap();
    }

    #[tokio::test]
    async fn issue_54_room_stops_after_the_game_reaches_a_terminal_state() {
        let (handle, task) = Room::new(near_terminal_state(), db()).spawn();
        let (tx0, mut rx0) = view_channel();
        handle.send(RoomInput::Join {
            seat: 0,
            outbox: tx0,
        });
        let opening = wait_for_view(&mut rx0).await;

        // Seat 0 holds priority. Passing runs state-based actions, which mark the
        // 0-life opponent as lost: the game becomes terminal.
        let pass = opening
            .valid_actions
            .iter()
            .find(|a| a.kind == "pass_priority")
            .expect("pass offered to the priority holder");
        handle.send(RoomInput::Message {
            seat: 0,
            message: ClientMessage::ChooseAction(ChooseAction {
                action_id: pass.id.clone(),
                ..Default::default()
            }),
        });

        // The room pushes exactly one final broadcast, then shuts down: seat 0
        // receives the terminal view and its outbox then closes.
        let _final_view = wait_for_view(&mut rx0).await;
        assert!(
            rx0.changed().await.is_err(),
            "the room's outbox should close once the game is over",
        );

        // The task terminates on its own, without any handle being dropped.
        task.await
            .expect("room task should terminate after game over");
    }

    #[tokio::test]
    async fn issue_119_final_broadcast_carries_the_game_result() {
        // On game over the room's final broadcast carries the terminal result, so a
        // client learns the winner and reason from the last view (CR 104.2a). While
        // the game is live the result is absent.
        let (handle, task) = Room::new(near_terminal_state(), db()).spawn();
        let (tx0, mut rx0) = view_channel();
        handle.send(RoomInput::Join {
            seat: 0,
            outbox: tx0,
        });
        let opening = wait_for_view(&mut rx0).await;
        assert!(opening.result.is_none(), "a live game carries no result");

        let pass = opening
            .valid_actions
            .iter()
            .find(|a| a.kind == "pass_priority")
            .expect("pass offered to the priority holder");
        handle.send(RoomInput::Message {
            seat: 0,
            message: ClientMessage::ChooseAction(ChooseAction {
                action_id: pass.id.clone(),
                ..Default::default()
            }),
        });

        // The single pass runs the SBA that marks the 0-life opponent lost; the
        // final broadcast then carries the terminal result.
        let final_view = wait_for_view(&mut rx0).await;
        let result = final_view
            .result
            .expect("the final broadcast carries the game result");
        assert_eq!(result.winner.as_deref(), Some("p0"));
        assert_eq!(result.reason, rune_protocol::GameOverReason::LifeZero);
        assert!(
            final_view.valid_actions.is_empty(),
            "a terminal view offers no actions"
        );

        task.await
            .expect("room task should terminate after game over");
    }

    /// A slow reader that pauses while the game advances must, on resuming, observe
    /// the *latest* view — intermediate superseded views are coalesced away and the
    /// outbox never accumulates a backlog. Exercises the per-seat `watch` outbox.
    #[tokio::test]
    async fn issue_57_slow_reader_coalesces_to_the_latest_view() {
        let (handle, task) = Room::new(GameState::new_two_player(), db()).spawn();
        let (tx0, mut rx0) = view_channel();
        let (tx1, mut rx1) = view_channel();
        handle.send(RoomInput::Join {
            seat: 0,
            outbox: tx0,
        });
        handle.send(RoomInput::Join {
            seat: 1,
            outbox: tx1,
        });

        // Seat 0 reads its opening view (holds priority), then becomes a *slow
        // reader*: it stops draining rx0 for the rest of the exchange. Seat 1 stays
        // responsive and doubles as our synchronization barrier.
        let opening0 = wait_for_view(&mut rx0).await;
        let _ = wait_for_view(&mut rx1).await;
        let pass0 = opening0
            .valid_actions
            .iter()
            .find(|a| a.kind == "pass_priority")
            .expect("pass offered to the priority holder");
        handle.send(RoomInput::Message {
            seat: 0,
            message: ClientMessage::ChooseAction(ChooseAction {
                action_id: pass0.id.clone(),
                ..Default::default()
            }),
        });

        // Seat 0 now pauses. Seat 1 receives priority and passes in turn; this pushes
        // *two* fresh views to the paused seat 0 (first "lost priority", then
        // "regained priority after the step advanced").
        let mut after0_seat1 = wait_for_view(&mut rx1).await;
        while after0_seat1.priority_player.as_deref() != Some("p1") {
            after0_seat1 = wait_for_view(&mut rx1).await;
        }
        let pass1 = after0_seat1
            .valid_actions
            .iter()
            .find(|a| a.kind == "pass_priority")
            .expect("priority handed to seat 1");
        handle.send(RoomInput::Message {
            seat: 1,
            message: ClientMessage::ChooseAction(ChooseAction {
                action_id: pass1.id.clone(),
                ..Default::default()
            }),
        });

        // Barrier: wait until seat 1 observes priority returning to p0. By then the
        // room has already written the latest view to seat 0's (paused) outbox too.
        let mut seat1_latest = wait_for_view(&mut rx1).await;
        while seat1_latest.priority_player.as_deref() != Some("p0") {
            seat1_latest = wait_for_view(&mut rx1).await;
        }

        // Seat 0 *resumes*. It must skip the intermediate "lost priority" snapshot
        // and read exactly the newest state (priority back to p0). If the outbox had
        // queued views, the first read here would be the stale no-priority view.
        let resumed0 = wait_for_view(&mut rx0).await;
        assert_eq!(resumed0.priority_player.as_deref(), Some("p0"));
        assert!(
            !resumed0.valid_actions.is_empty(),
            "coalesced view is the latest, in which seat 0 holds priority again",
        );
        // Bounded depth: a single latest value, no backlog left to drain.
        assert!(
            !rx0.has_changed().unwrap(),
            "the outbox coalesces to one latest view, never a queue of superseded ones",
        );

        drop(handle);
        task.await.unwrap();
    }

    /// A flooding client cannot grow the room inbox without bound: once the bounded
    /// queue is full, excess inputs are dropped and the room stays alive. Fills the
    /// inbox directly (no consumer) so exactly [`ROOM_INBOX_CAPACITY`] inputs are
    /// retained regardless of how many are sent.
    #[tokio::test]
    async fn issue_57_flooding_client_inbox_is_bounded_and_excess_is_dropped() {
        let (inbox_tx, mut inbox_rx) = mpsc::channel::<RoomInput>(ROOM_INBOX_CAPACITY);
        let handle = RoomHandle { inbox: inbox_tx };

        // No room task drains the inbox: every accepted input stays buffered. Filling
        // to capacity succeeds and the room (receiver) is still alive throughout.
        for _ in 0..ROOM_INBOX_CAPACITY {
            assert!(
                handle.send(RoomInput::Leave { seat: 0 }),
                "delivery within capacity keeps the room alive",
            );
        }
        // The queue is now full. A flood of further inputs is dropped rather than
        // buffered; each still reports the room as alive (not a disconnect).
        for _ in 0..(ROOM_INBOX_CAPACITY * 4) {
            assert!(
                handle.send(RoomInput::Leave { seat: 0 }),
                "excess inputs are dropped, not treated as a closed room",
            );
        }

        // Exactly the capacity was retained — memory is bounded no matter the flood.
        let mut buffered = 0;
        while inbox_rx.try_recv().is_ok() {
            buffered += 1;
        }
        assert_eq!(
            buffered, ROOM_INBOX_CAPACITY,
            "the inbox never grows past its bound under a flood",
        );

        // Once the room's receiver is gone, delivery reports the room as stopped.
        drop(inbox_rx);
        assert!(!handle.send(RoomInput::Leave { seat: 0 }));
    }
}
