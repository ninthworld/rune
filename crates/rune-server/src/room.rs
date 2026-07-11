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

use std::future::Future;

use futures_util::{SinkExt, StreamExt};
use rune_engine::{apply_action, CardDatabase, GameState, PlayerId};
use rune_protocol::{ClientMessage, GameView};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;
use tracing::{info, warn};

use crate::view::{personalized_view, resolve_action};

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
        /// Channel the room pushes this seat's views to.
        outbox: mpsc::UnboundedSender<GameView>,
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
    inbox: mpsc::UnboundedSender<RoomInput>,
}

impl RoomHandle {
    /// Deliver an input to the room. Returns `false` if the room task has already
    /// stopped (its receiver was dropped), so callers can give up cleanly.
    pub fn send(&self, input: RoomInput) -> bool {
        self.inbox.send(input).is_ok()
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
    /// disconnected (held open).
    seats: Vec<Option<mpsc::UnboundedSender<GameView>>>,
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
        let (inbox_tx, inbox_rx) = mpsc::unbounded_channel();
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
    pub async fn run(mut self, mut inbox: mpsc::UnboundedReceiver<RoomInput>) {
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

    /// Whether the game has reached a terminal state: some player has lost.
    ///
    /// The engine sets `Player::has_lost` in its state-based-actions loop
    /// (`crates/rune-engine/src/player.rs`); the room only reads that flag and never
    /// decides a loss itself, keeping all game logic in the engine.
    fn game_over(&self) -> bool {
        self.state.players.iter().any(|player| player.has_lost)
    }

    /// Seat (or re-seat) a connection and bring it current with a full view.
    fn on_join(&mut self, seat: Seat, outbox: mpsc::UnboundedSender<GameView>) {
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
                match resolve_action(&self.state, &self.db, PlayerId(seat), &choose.action_id) {
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

    /// Push the seat's freshly-personalized view to its outbox. If the receiver is
    /// gone, treat it as a disconnect and hold the seat open.
    fn send_view(&mut self, seat: Seat) {
        let view = personalized_view(&self.state, &self.db, PlayerId(seat));
        let Some(slot) = self.seats.get_mut(seat) else {
            return;
        };
        let Some(outbox) = slot.as_ref() else {
            return;
        };
        if outbox.send(view).is_err() {
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
    let (outbox_tx, mut outbox_rx) = mpsc::unbounded_channel::<GameView>();
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
            view = outbox_rx.recv() => match view {
                Some(view) => match serde_json::to_string(&view) {
                    Ok(json) => {
                        if write.send(Message::Text(json)).await.is_err() {
                            break;
                        }
                    }
                    Err(error) => warn!(seat, %error, "failed to serialize game view"),
                },
                // The room dropped our outbox (task stopped): nothing more to send.
                None => break,
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
    use tokio::sync::mpsc::error::TryRecvError;

    fn db() -> CardDatabase {
        CardDatabase::bundled().unwrap()
    }

    /// Receive one view, awaiting the room task rather than busy-polling.
    async fn wait_for_view(rx: &mut mpsc::UnboundedReceiver<GameView>) -> GameView {
        rx.recv().await.expect("room should push a view")
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
        let (tx0, mut rx0) = mpsc::unbounded_channel();
        let (tx1, mut rx1) = mpsc::unbounded_channel();
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
        let (tx0, mut rx0) = mpsc::unbounded_channel();
        let (tx1, mut rx1) = mpsc::unbounded_channel();
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
        let (tx0, mut rx0) = mpsc::unbounded_channel();
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
        let (tx0, mut rx0) = mpsc::unbounded_channel();
        let (tx1, mut rx1) = mpsc::unbounded_channel();
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
        // Seat 0 was never re-broadcast because nothing changed.
        assert!(matches!(rx0.try_recv(), Err(TryRecvError::Empty)));

        drop(handle);
        task.await.unwrap();
    }

    #[tokio::test]
    async fn reconnect_is_brought_current_with_a_full_view() {
        let (handle, task) = Room::new(dealt_state(), db()).spawn();
        let (tx0, mut rx0) = mpsc::unbounded_channel();
        handle.send(RoomInput::Join {
            seat: 0,
            outbox: tx0,
        });
        let _ = wait_for_view(&mut rx0).await;

        // Disconnect: the seat is held open, the game is untouched.
        handle.send(RoomInput::Leave { seat: 0 });

        // Reconnect with a fresh outbox: the room re-sends the latest full view.
        let (tx0b, mut rx0b) = mpsc::unbounded_channel();
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
        let (tx0, mut rx0) = mpsc::unbounded_channel();
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
            rx0.recv().await.is_none(),
            "the room's outbox should close once the game is over",
        );

        // The task terminates on its own, without any handle being dropped.
        task.await
            .expect("room task should terminate after game over");
    }
}
