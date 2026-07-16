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
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use rune_engine::{apply_action, valid_actions, Action, CardDatabase, GameState, PlayerId};
use rune_protocol::{ClientMessage, GameView};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tokio::time::Instant;
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

/// A room's decision-timer policy (issue #263).
///
/// The engine is pure and timer-free (ADR 0002); deadline policy and enforcement
/// live here in the room layer, which already owns tokio time. Timers are **off by
/// default** — an off policy reproduces exactly the pre-timer behavior, so existing
/// flows and tests are unchanged — and, when on, apply only to in-game decisions;
/// the lobby/deck-submission phase is explicitly out of scope (a room only exists
/// once a game has been constructed).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TimerPolicy {
    /// No decision clock: a seat may take as long as it likes (the default, and the
    /// behavior before timers existed).
    #[default]
    Off,
    /// Each in-game decision must be answered within `limit`; on expiry the room
    /// takes a conservative default action on the deciding player's behalf (see
    /// [`timeout_default_action`]).
    PerDecision {
        /// How long the deciding player has before the default action fires.
        limit: Duration,
    },
}

/// The conservative default action the room takes when a decision times out
/// (issue #263). This is deliberately a *safe no-op-ish* choice, never a
/// game-losing one — a single missed prompt must not concede (CR 104.3a is reserved
/// for an explicit concession or a future idle-escalation policy):
///
/// - In an ordinary priority window, **pass priority** — the universal safe default.
/// - For a forced combat declaration, declare **no** attackers/blockers (CR 508.1a /
///   509.1a both allow the empty declaration).
/// - Any other forced decision (mulligan keep/mulligan, cleanup discard) has no safe
///   auto-answer, so the timer does not force it — the room stops the clock for that
///   decision rather than guess (idle-escalation is future work). Returns `None`.
///
/// All legality is still enforced by [`apply_action`]; this only picks *which*
/// offered action to take, reading the engine's own [`valid_actions`].
fn timeout_default_action(state: &GameState, db: &CardDatabase) -> Option<Action> {
    let actions = valid_actions(state, db);
    if actions.iter().any(|a| matches!(a, Action::PassPriority)) {
        return Some(Action::PassPriority);
    }
    if actions
        .iter()
        .any(|a| matches!(a, Action::DeclareAttackers { .. }))
    {
        return Some(Action::DeclareAttackers {
            attackers: Vec::new(),
        });
    }
    if actions
        .iter()
        .any(|a| matches!(a, Action::DeclareBlockers { .. }))
    {
        return Some(Action::DeclareBlockers { blocks: Vec::new() });
    }
    None
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
///
/// # Decision timer (issue #263)
/// A room optionally runs a per-decision clock ([`TimerPolicy`], off by default).
/// The deadline is an **absolute** instant, so a reconnecting client is re-sent the
/// correct seconds-remaining rather than a fresh clock — the timer does not reset on
/// reconnect. On expiry the room applies a conservative default action on the
/// deciding player's behalf ([`timeout_default_action`]); a single missed prompt
/// never concedes.
pub struct Room {
    state: GameState,
    db: CardDatabase,
    /// Per-seat outbox, indexed by [`Seat`]. `None` means the seat is currently
    /// disconnected (held open). Each present sender is the [`watch`] half of a
    /// latest-value channel, so pushing a view never blocks the room nor buffers
    /// superseded snapshots.
    seats: Vec<Option<watch::Sender<Option<GameView>>>>,
    /// The decision-timer policy (issue #263). [`TimerPolicy::Off`] by default.
    timer: TimerPolicy,
    /// The absolute deadline for the current decision, if a clock is running. Set
    /// when a fresh decision is presented (after any applied action) and read to
    /// project `action_deadline` into the deciding seat's view. Absolute, so a
    /// reconnect re-send reflects the real remaining time rather than restarting it.
    deadline: Option<Instant>,
}

impl Room {
    /// Create a room around an initial `state` and card `db`. The number of seats
    /// is fixed by `state.players`; each seat starts disconnected. Timers are off;
    /// use [`Room::with_timer_policy`] to enable a decision clock.
    #[must_use]
    pub fn new(state: GameState, db: CardDatabase) -> Self {
        let seats = state.players.iter().map(|_| None).collect();
        Self {
            state,
            db,
            seats,
            timer: TimerPolicy::Off,
            deadline: None,
        }
    }

    /// Set this room's decision-timer policy (issue #263). Chainable on
    /// [`Room::new`]; the default is [`TimerPolicy::Off`].
    #[must_use]
    pub fn with_timer_policy(mut self, policy: TimerPolicy) -> Self {
        self.timer = policy;
        self
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
        // Start the clock on the opening decision (a no-op when timers are off).
        self.arm_deadline();
        while !self.game_over() {
            // Copy the deadline out so the timer future borrows nothing of `self`
            // (the input arm needs `&mut self`). A `None` deadline parks forever, so
            // the timer arm simply never fires when no clock is running.
            let deadline = self.deadline;
            tokio::select! {
                maybe = inbox.recv() => {
                    let Some(input) = maybe else {
                        info!("room input channel closed; room task stopping");
                        return;
                    };
                    match input {
                        RoomInput::Join { seat, outbox } => self.on_join(seat, outbox),
                        RoomInput::Message { seat, message } => self.on_message(seat, &message),
                        RoomInput::Leave { seat } => self.on_leave(seat),
                    }
                }
                () = async move {
                    match deadline {
                        Some(at) => tokio::time::sleep_until(at).await,
                        None => std::future::pending::<()>().await,
                    }
                } => {
                    self.on_timeout();
                }
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
                        // The decision advanced: restart the clock for whatever the
                        // new state presents next (a no-op when timers are off).
                        self.arm_deadline();
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

    /// (Re)arm the decision clock for the state the room now presents (issue #263).
    ///
    /// Sets an absolute deadline `limit` from now when a timer policy is active and a
    /// decision is actually pending; clears it otherwise. Called after every applied
    /// action (a fresh decision) and at room start — but never on join/leave, so a
    /// reconnect observes the real remaining time rather than restarting the clock.
    fn arm_deadline(&mut self) {
        self.deadline = match self.timer {
            TimerPolicy::PerDecision { limit } if self.decision_pending() => {
                Some(Instant::now() + limit)
            }
            _ => None,
        };
    }

    /// Whether a live in-game decision is pending — the game is not over and some
    /// seat holds priority (and so has actions to take). The clock only runs while
    /// this holds.
    fn decision_pending(&self) -> bool {
        !self.game_over() && self.state.priority_holder().is_some()
    }

    /// The decision clock expired (issue #263): apply the conservative default action
    /// on the deciding player's behalf, then restart the clock for the next decision.
    ///
    /// If there is no safe default (mulligan/discard) or the default would be a
    /// no-op, the clock is left cleared for this decision rather than hot-looping —
    /// the decision then waits for the player (or a future idle-escalation policy).
    fn on_timeout(&mut self) {
        if let Some(action) = timeout_default_action(&self.state, &self.db) {
            let next = apply_action(&self.state, &action, &self.db);
            if next != self.state {
                self.state = next;
                self.arm_deadline();
                if !self.game_over() {
                    self.broadcast();
                }
                return;
            }
        }
        // Nothing safe to do automatically: stop timing this decision.
        self.deadline = None;
    }

    /// Push the seat's freshly-personalized view to its outbox. Writing to the
    /// latest-value [`watch`] never blocks and overwrites any view the reader has
    /// not yet consumed (coalescing to newest). If the receiver is gone, treat it as
    /// a disconnect and hold the seat open.
    ///
    /// When a decision clock is running (issue #263), the deciding seat's view — the
    /// one with actions on offer — carries `action_deadline` as the seconds remaining
    /// until the default action fires, computed from the absolute deadline so a
    /// reconnect sees the true remaining time.
    fn send_view(&mut self, seat: Seat) {
        let mut view = personalized_view(&self.state, &self.db, PlayerId(seat));
        if let Some(at) = self.deadline {
            if !view.valid_actions.is_empty() {
                view.action_deadline =
                    Some(at.saturating_duration_since(Instant::now()).as_secs_f64());
            }
        }
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
    use crate::test_support::fixture;
    use rune_engine::{GameState, Step};
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
        let p0_hand = vec![
            state.new_instance(fixture("forest")),
            state.new_instance(fixture("verdant_scout")),
        ];
        let p0_lib = vec![state.new_instance(fixture("thornback_boar"))];
        let p1_hand = vec![state.new_instance(fixture("thornback_boar"))];
        let p1_lib = vec![
            state.new_instance(fixture("thornback_boar")),
            state.new_instance(fixture("thornback_boar")),
        ];
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

    // ----- Decision timers (issue #263) -----

    /// A [`dealt_state`] whose player 1 sits at 0 life, so the very next applied
    /// action (a timeout's default pass) runs state-based actions that end the game —
    /// giving the auto-advancing test clock a terminal state to stop at.
    fn near_terminal_dealt_state() -> GameState {
        let mut state = dealt_state();
        state.players[1].life = 0;
        state
    }

    #[tokio::test]
    async fn issue_263_timers_off_by_default_leave_no_deadline() {
        // The default policy reproduces the pre-timer behavior: no `action_deadline`
        // rides the view, so existing flows are unchanged.
        let (handle, task) = Room::new(dealt_state(), db()).spawn();
        let (tx0, mut rx0) = view_channel();
        handle.send(RoomInput::Join {
            seat: 0,
            outbox: tx0,
        });
        let view0 = wait_for_view(&mut rx0).await;
        assert!(!view0.valid_actions.is_empty(), "seat 0 is the decider");
        assert!(
            view0.action_deadline.is_none(),
            "no clock runs under the default (off) policy"
        );
        drop(handle);
        task.await.unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn issue_263_timer_projects_a_deadline_that_survives_reconnect() {
        // With a per-decision clock, the deciding seat's view carries the seconds
        // remaining. The deadline is absolute, so a reconnect midway through sees the
        // reduced remaining time rather than a fresh clock.
        let policy = TimerPolicy::PerDecision {
            limit: Duration::from_secs(30),
        };
        let (handle, task) = Room::new(dealt_state(), db())
            .with_timer_policy(policy)
            .spawn();
        let (tx0, mut rx0) = view_channel();
        handle.send(RoomInput::Join {
            seat: 0,
            outbox: tx0,
        });
        let view0 = wait_for_view(&mut rx0).await;
        let first = view0.action_deadline.expect("the decider is on the clock");
        assert!(
            (29.0..=30.0).contains(&first),
            "roughly the full limit remains at the start: {first}"
        );

        // Ten seconds pass without an action, then seat 0 reconnects. The re-sent
        // view reflects ~20s left — the clock did not restart.
        tokio::time::advance(Duration::from_secs(10)).await;
        let (tx0b, mut rx0b) = view_channel();
        handle.send(RoomInput::Join {
            seat: 0,
            outbox: tx0b,
        });
        let reconnect = wait_for_view(&mut rx0b).await;
        let remaining = reconnect.action_deadline.expect("still on the clock");
        assert!(
            (19.0..=21.0).contains(&remaining),
            "reconnect keeps the absolute deadline, ~20s left: {remaining}"
        );

        drop(handle);
        task.await.unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn issue_263_timer_expiry_takes_the_default_action() {
        // Nobody acts; when the clock runs out the room takes the safe default
        // (pass priority) on the deciding seat's behalf. Here that pass runs
        // state-based actions that finish the game (player 1 was at 0 life), so the
        // effect is observable as the terminal result on the final broadcast — and
        // the auto-advancing paused clock has a terminal state to stop at.
        let policy = TimerPolicy::PerDecision {
            limit: Duration::from_secs(30),
        };
        let (handle, task) = Room::new(near_terminal_dealt_state(), db())
            .with_timer_policy(policy)
            .spawn();
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
        let view0 = wait_for_view(&mut rx0).await;
        let _view1 = wait_for_view(&mut rx1).await;
        assert!(
            view0.action_deadline.is_some(),
            "seat 0 is on the clock and no one has acted"
        );

        // The paused runtime auto-advances to the deadline; the default pass fires,
        // the game ends, and the room's final broadcast carries the result.
        let terminal = wait_for_view(&mut rx0).await;
        assert!(
            terminal.result.is_some(),
            "the timed-out default action drove the game to a terminal state"
        );
        // The task stops on its own once terminal.
        task.await.unwrap();
        drop(handle);
    }
}
