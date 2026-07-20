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
use rune_engine::{
    apply_action, attackers_needing_damage_order, priority_has_no_meaningful_action, valid_actions,
    Action, CardDatabase, DamageOrder, GameState, PlayerId,
};
use rune_protocol::{ClientMessage, GameView, Phase, SetStops, SpectatorView};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tokio::time::Instant;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;
use tracing::{info, warn};

use crate::view::{personalized_view, phase_of, resolve_action, spectator_view};

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
    /// A **spectator** connection attached (ADR 0022, issue #351): a non-seated
    /// observer. `outbox` is where the room pushes redacted [`SpectatorView`]s; the
    /// room immediately sends the current one so a mid-game spectator reconstructs the
    /// whole public board from a single message. A spectator owns no seat and is not
    /// held open — its sender is simply dropped from the fan-out when the connection
    /// ends (detected on the next broadcast).
    JoinSpectator {
        /// Latest-value channel the room pushes redacted spectator views to.
        outbox: watch::Sender<Option<SpectatorView>>,
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

    /// Whether the room task is still running — its input channel is open. The task
    /// drops its receiver when the game reaches a terminal state (or is otherwise
    /// stopped), so this returns `false` once the game is over. The lobby uses it to
    /// prune a finished room from the public directory (issue #280) and reclaim its
    /// capacity, since the pure engine gives the lobby no other game-over signal.
    #[must_use]
    pub(crate) fn is_active(&self) -> bool {
        !self.inbox.is_closed()
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
    if actions
        .iter()
        .any(|a| matches!(a, Action::OrderCombatDamage { .. }))
    {
        // Combat-damage assignment order (issue #346): resolve to the deterministic
        // battlefield-order default — the exact assignment used before player choice
        // existed — so an unattended game never stalls and never concedes.
        let orders = attackers_needing_damage_order(state)
            .into_iter()
            .map(|attacker| DamageOrder {
                attacker,
                blockers: state
                    .battlefield
                    .iter()
                    .filter(|p| p.blocking == Some(attacker))
                    .map(|p| p.id)
                    .collect(),
            })
            .collect();
        return Some(Action::OrderCombatDamage { orders });
    }
    None
}

/// A room's basic priority-automation policy (issue #264, ADR 0020).
///
/// Like [`TimerPolicy`], automation is a room-layer concern layered over the pure,
/// automation-free engine: the engine only *reports* (via
/// [`priority_has_no_meaningful_action`]) whether the priority holder has a
/// meaningful action; the room owns the loop that acts on it. **Off by default** —
/// an off policy reproduces exactly the pre-automation behavior, so every existing
/// flow and test is unchanged — and, when on, auto-passes a seat's priority while
/// the engine says it is idle and the seat has not opted to stop at the current step
/// (its `set_stops` preferences, held per seat on the room).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum AutoPassPolicy {
    /// No automation: every priority pass is manual (the default, and the behavior
    /// before automation existed).
    #[default]
    Off,
    /// Auto-pass an idle seat's priority (per its stop preferences).
    On,
}

/// A hard cap on how many priority passes one settle may apply, a defence against a
/// pathological stop configuration that never reaches a meaningful decision. The
/// loop terminates naturally far below this every turn (the active player's
/// declare-attackers step is a forced choice that offers no pass), so hitting the
/// cap signals a bug; it is logged and the settle stops rather than hanging the task.
const MAX_AUTO_PASSES: usize = 256;

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
    /// Each seat's public display name in seat order (issue #294), or `None` for an
    /// unnamed seat. Names are a *session*/lobby concern, not engine state, so the
    /// room carries them here and projects them into every seat's
    /// [`GameView::player_names`] rather than the pure [`personalized_view`] shim.
    /// Empty (all-`None`) when no name was ever set, so the map elides from the wire.
    player_names: Vec<Option<String>>,
    /// The basic priority-automation policy (issue #264). [`AutoPassPolicy::Off`] by
    /// default, so automation is opt-in and existing behavior is unchanged.
    auto_pass: AutoPassPolicy,
    /// Each seat's priority-stop preferences in seat order (issue #264, ADR 0020):
    /// the steps at which that seat wants priority even when the engine reports no
    /// meaningful action, so auto-pass does not skip it there. Set over the protocol
    /// (`set_stops`) and held here — like [`Self::player_names`], a per-seat concern
    /// that is *not* engine state — so a preference survives a disconnect/reconnect
    /// (the room is never torn down on leave). Empty per seat by default (stop
    /// nowhere); reflected back in each seat's [`GameView::stops`].
    stops: Vec<Vec<Phase>>,
    /// Which seats were auto-passed during the most recent settle (issue #264): a
    /// transient, display-only signal, recomputed each settle and projected into the
    /// affected seat's [`GameView::auto_passed`] on the following broadcast so a
    /// client can show a "passed for you" indicator. Not load-bearing state.
    auto_passed_seats: Vec<bool>,
    /// The connected **spectators** (ADR 0022, issue #351): each a latest-value sender
    /// the room pushes a redacted [`SpectatorView`] to on every broadcast. Spectators
    /// own no seat and are not held open across disconnects — a sender whose receiver
    /// has been dropped is pruned on the next broadcast. Empty by default, so a room
    /// with no spectators does exactly the seated work it did before.
    spectators: Vec<watch::Sender<Option<SpectatorView>>>,
}

impl Room {
    /// Create a room around an initial `state` and card `db`. The number of seats
    /// is fixed by `state.players`; each seat starts disconnected. Timers are off;
    /// use [`Room::with_timer_policy`] to enable a decision clock.
    #[must_use]
    pub fn new(state: GameState, db: CardDatabase) -> Self {
        let seat_count = state.players.len();
        let seats = (0..seat_count).map(|_| None).collect();
        Self {
            state,
            db,
            seats,
            timer: TimerPolicy::Off,
            deadline: None,
            player_names: Vec::new(),
            auto_pass: AutoPassPolicy::Off,
            stops: vec![Vec::new(); seat_count],
            auto_passed_seats: vec![false; seat_count],
            spectators: Vec::new(),
        }
    }

    /// Set this room's decision-timer policy (issue #263). Chainable on
    /// [`Room::new`]; the default is [`TimerPolicy::Off`].
    #[must_use]
    pub fn with_timer_policy(mut self, policy: TimerPolicy) -> Self {
        self.timer = policy;
        self
    }

    /// Set this room's priority-automation policy (issue #264). Chainable on
    /// [`Room::new`]; the default is [`AutoPassPolicy::Off`].
    #[must_use]
    pub fn with_auto_pass(mut self, policy: AutoPassPolicy) -> Self {
        self.auto_pass = policy;
        self
    }

    /// Preset each seat's priority-stop preferences (issue #264), indexed by seat.
    /// Chainable on [`Room::new`]; the default is no stops for any seat. A seat with
    /// an index past the end of `stops` keeps its empty default. In production the
    /// preferences arrive over the wire (`set_stops`); this seeds them for tests.
    #[must_use]
    pub fn with_stops(mut self, stops: Vec<Vec<Phase>>) -> Self {
        for (seat, set) in stops.into_iter().enumerate() {
            if let Some(slot) = self.stops.get_mut(seat) {
                *slot = set;
            }
        }
        self
    }

    /// Set the per-seat display names this room labels players with (issue #294),
    /// indexed by seat. Chainable on [`Room::new`]; the default is no names (every
    /// seat unnamed), so `GameView::player_names` stays empty and elides from the wire.
    /// A seat with `None`, or an index past the end of `names`, simply has no name.
    #[must_use]
    pub fn with_player_names(mut self, names: Vec<Option<String>>) -> Self {
        self.player_names = names;
        self
    }

    /// The public display-name map for a `GameView` (issue #294): every seat that has
    /// a name, keyed by its `p{N}` player id. Empty when no seat is named, so the field
    /// elides from the wire and older-server behavior is preserved.
    fn player_names_map(&self) -> std::collections::BTreeMap<String, String> {
        self.player_names
            .iter()
            .enumerate()
            .filter_map(|(seat, name)| name.as_ref().map(|n| (format!("p{seat}"), n.clone())))
            .collect()
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
        // Fast-forward any idle opening priority (a no-op when automation is off),
        // then start the clock on whatever decision actually rests (issue #264).
        self.settle_auto_passes();
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
                        RoomInput::JoinSpectator { outbox } => self.on_join_spectator(outbox),
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

    /// Attach a spectator (ADR 0022, issue #351) and bring it current with a single
    /// redacted [`SpectatorView`] — the whole public board, so a mid-game spectator
    /// reconstructs its UI with no history. A spectator owns no seat and never mutates
    /// the game; a dead spectator sender is pruned lazily on the next broadcast.
    fn on_join_spectator(&mut self, outbox: watch::Sender<Option<SpectatorView>>) {
        let mut view = spectator_view(&self.state, &self.db);
        view.player_names = self.player_names_map();
        // If the receiver is already gone, don't retain the sender.
        if outbox.send(Some(view)).is_ok() {
            self.spectators.push(outbox);
        }
    }

    /// Push the current redacted [`SpectatorView`] to every connected spectator,
    /// pruning any whose receiver has been dropped (the spectator disconnected). A
    /// no-op when there are no spectators, so a seated-only room is unaffected.
    fn broadcast_spectators(&mut self) {
        if self.spectators.is_empty() {
            return;
        }
        let mut view = spectator_view(&self.state, &self.db);
        view.player_names = self.player_names_map();
        self.spectators
            .retain(|outbox| outbox.send(Some(view.clone())).is_ok());
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
                        // Auto-pass any idle priority the action left behind (a no-op
                        // when automation is off), then restart the clock for whatever
                        // decision now rests (a no-op when timers are off).
                        self.settle_auto_passes();
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
                        // Re-send the unchanged view flagged as a rejection (issue #265)
                        // so the client can show a brief, non-blaming "the game moved on"
                        // notice. With a `valid_actions`-driven client this is a rare
                        // stale-view race, not a user error.
                        self.send_view_flagged(seat, true);
                    }
                }
            }
            ClientMessage::SetStops(set) => self.on_set_stops(seat, set),
        }
    }

    /// Record a seat's priority-stop preferences (issue #264, ADR 0020) and reflect
    /// them back. The preferences are held on the room, like the display name, so
    /// they survive reconnect; a stops change can make the current priority holder
    /// newly eligible to auto-pass (they cleared a stop), so a settle runs, and the
    /// clock is re-armed only if that settle actually advanced the game.
    fn on_set_stops(&mut self, seat: Seat, set: &SetStops) {
        let Some(slot) = self.stops.get_mut(seat) else {
            warn!(seat, "set_stops for a seat that does not exist; ignoring");
            return;
        };
        // Replace the seat's set wholesale, de-duplicated so the reflected list is
        // canonical (a client that sends the same phase twice sees it once back).
        let mut stops = set.stops.clone();
        stops.dedup();
        *slot = stops;
        let advanced = self.settle_auto_passes();
        if advanced {
            self.arm_deadline();
        }
        if !self.game_over() {
            self.broadcast();
        }
    }

    /// Auto-pass the priority holder while it is idle and has not opted to stop at the
    /// current step (issue #264, ADR 0020). Returns whether any pass was applied.
    ///
    /// A no-op unless [`AutoPassPolicy::On`]. Each iteration passes priority for
    /// whichever seat currently holds it — the engine's own `PassPriority`, so the
    /// resulting state is identical to a manual pass and determinism is preserved.
    /// The loop stops the instant a seat has a meaningful action, owes a forced choice
    /// (a window with no pass on offer — e.g. the active player's declare-attackers),
    /// or has opted to stop; a fixed [`MAX_AUTO_PASSES`] cap is a defensive backstop
    /// so a pathological configuration can never hang the task.
    fn settle_auto_passes(&mut self) -> bool {
        for flag in &mut self.auto_passed_seats {
            *flag = false;
        }
        if self.auto_pass != AutoPassPolicy::On {
            return false;
        }
        let mut advanced = false;
        let mut passes = 0usize;
        loop {
            if self.game_over() || self.state.priority_holder().is_none() {
                break;
            }
            let seat = self.state.priority.0;
            if !self.should_auto_pass(seat) {
                break;
            }
            if passes >= MAX_AUTO_PASSES {
                // Still idle after the cap: a stop configuration that never rests. Log
                // it and stop; the game waits for a human rather than the task spinning.
                warn!("auto-pass settle hit its cap without reaching a decision; stopping");
                break;
            }
            let next = apply_action(&self.state, &Action::PassPriority, &self.db);
            // Defensive: a pass that does not change state would loop forever.
            if next == self.state {
                break;
            }
            self.state = next;
            if let Some(flag) = self.auto_passed_seats.get_mut(seat) {
                *flag = true;
            }
            advanced = true;
            passes += 1;
        }
        advanced
    }

    /// Whether `seat`, which currently holds priority, should be auto-passed: the
    /// engine reports it has no meaningful action **and** the seat has not opted to
    /// stop at the current step (issue #264). The engine predicate is the rules
    /// authority (the client may not make this call); the stop set is the seat's
    /// opt-in escape hatch.
    fn should_auto_pass(&self, seat: Seat) -> bool {
        if !priority_has_no_meaningful_action(&self.state, &self.db) {
            return false;
        }
        let here = phase_of(self.state.step);
        !self
            .stops
            .get(seat)
            .is_some_and(|stops| stops.contains(&here))
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
                // The default action can leave idle priority behind, exactly as a
                // player's action does; settle it before re-arming (a no-op when
                // automation is off).
                self.settle_auto_passes();
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
        self.send_view_flagged(seat, false);
    }

    /// Send `seat` its personalized view, flagging it as the response to a **rejected
    /// action** when `action_rejected` (issue #265). Only the rejection re-send in
    /// [`Self::on_message`] passes `true`; every other push (normal broadcast, join
    /// resync) goes through [`Self::send_view`] with `false`, so the transient
    /// "the game moved on" notice fires once and is never resurrected by a later resync.
    fn send_view_flagged(&mut self, seat: Seat, action_rejected: bool) {
        let mut view = personalized_view(&self.state, &self.db, PlayerId(seat));
        // Names are a lobby/session concern, not engine state, so the room labels
        // players here rather than in the pure projection shim (issue #294).
        view.player_names = self.player_names_map();
        // Priority-stop preferences and the auto-pass indicator are likewise room
        // state, not engine state, and per-viewer (issue #264): reflect this seat's
        // stops so its stops UI is reconstructable, and flag whether reaching this
        // state auto-passed it.
        view.stops = self.stops.get(seat).cloned().unwrap_or_default();
        view.auto_passed = self.auto_passed_seats.get(seat).copied().unwrap_or(false);
        // Rejected-action feedback (issue #265): the only caller that sets this is the
        // rejection re-send, and the game state is unchanged, so this rides an otherwise
        // ordinary resync — advisory presentation, never load-bearing.
        view.action_rejected = action_rejected;
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

    /// Send every connected seat its own personalized view, and every spectator the
    /// current redacted view. Seated traffic is exactly as before; the spectator
    /// fan-out is a no-op when there are no spectators (ADR 0022, issue #351).
    fn broadcast(&mut self) {
        for seat in 0..self.seats.len() {
            let connected = self.seats.get(seat).map(Option::is_some).unwrap_or(false);
            if connected {
                self.send_view(seat);
            }
        }
        self.broadcast_spectators();
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
    /// and a creature and player 1 holds a single card. Enough to exercise
    /// hidden-zone redaction and a real (non-pass) action.
    fn dealt_state() -> GameState {
        let mut state = GameState::new_two_player();
        state.step = Step::PrecombatMain;
        let p0_hand = vec![
            state.new_instance(fixture("forest")),
            state.new_instance(fixture("walking_corpse")),
        ];
        let p0_lib = vec![state.new_instance(fixture("onakke_ogre"))];
        let p1_hand = vec![state.new_instance(fixture("onakke_ogre"))];
        let p1_lib = vec![
            state.new_instance(fixture("onakke_ogre")),
            state.new_instance(fixture("onakke_ogre")),
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

    /// Receive the next (latest) spectator view, awaiting the room task.
    async fn wait_for_spectator_view(
        rx: &mut watch::Receiver<Option<SpectatorView>>,
    ) -> SpectatorView {
        rx.changed()
            .await
            .expect("room should push a spectator view");
        rx.borrow_and_update().clone().expect("a pushed view")
    }

    #[tokio::test]
    async fn a_spectator_joins_mid_game_and_receives_a_redacted_view() {
        // A seated game underway; a spectator attaches and immediately reconstructs the
        // whole public board from one SpectatorView — every seat as public counts, no
        // hand contents, and it keeps updating as the game advances (issue #351).
        let (handle, task) = Room::new(dealt_state(), db()).spawn();
        let (tx0, mut rx0) = view_channel();
        assert!(handle.send(RoomInput::Join {
            seat: 0,
            outbox: tx0
        }));
        let seat0_view = wait_for_view(&mut rx0).await;
        // Grab seat 0's pass action now (its view will not change on a spectator join).
        let action = seat0_view
            .valid_actions
            .iter()
            .find(|a| a.kind == "pass_priority")
            .cloned()
            .expect("a pass is offered to the priority holder");

        // A spectator attaches after the game is underway.
        let (stx, mut srx) = watch::channel::<Option<SpectatorView>>(None);
        assert!(handle.send(RoomInput::JoinSpectator { outbox: stx }));
        let spec = wait_for_spectator_view(&mut srx).await;

        // Every seat appears as a public OpponentView with only counts, no hand cards.
        assert_eq!(spec.players.len(), 2);
        assert_eq!(spec.players[0].hand_size, 2);
        assert_eq!(spec.players[1].hand_size, 1);
        // The public board is fully present (reconstruct-from-one-message).
        let json = serde_json::to_value(&spec).unwrap();
        assert!(json.get("valid_actions").is_none());
        assert!(json.get("my_hand").is_none());
        assert!(json.get("you").is_none());
        assert!(handle.send(RoomInput::Message {
            seat: 0,
            message: ClientMessage::ChooseAction(ChooseAction {
                action_id: action.id,
                token: action.token,
                targets: vec![],
            }),
        }));
        let updated = wait_for_spectator_view(&mut srx).await;
        // Still redacted, still every seat public — the update is a full public snapshot.
        assert_eq!(updated.players.len(), 2);

        drop(handle);
        task.await.unwrap();
    }

    #[tokio::test]
    async fn a_room_with_no_spectators_broadcasts_exactly_as_before() {
        // Zero-spectator rooms do the seated work unchanged: the spectator fan-out is a
        // no-op, so a seated pass round is byte-for-byte the two-player behavior.
        let (handle, task) = Room::new(dealt_state(), db()).spawn();
        let (tx0, mut rx0) = view_channel();
        assert!(handle.send(RoomInput::Join {
            seat: 0,
            outbox: tx0
        }));
        let view0 = wait_for_view(&mut rx0).await;
        assert_eq!(view0.you, "p0");
        assert!(!view0.valid_actions.is_empty());
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
        // …but it is flagged as a rejection so the client can surface the transient
        // "the game moved on" notice (issue #265). The initial view was not flagged.
        assert!(!before.action_rejected);
        assert!(resent.action_rejected);

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
        // The resync is flagged as a rejection for the sending seat (issue #265).
        assert!(resent.action_rejected);
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

    // ----- Basic priority automation (issue #264, ADR 0020) -----

    use rune_protocol::{Phase, SetStops};

    /// A two-player game where neither seat can ever take a meaningful action: empty
    /// hands and boards, and libraries of uncastable creatures (drawn cards can never
    /// be cast — no lands, no mana — so a seat stays idle every turn without decking).
    /// Starts at seat 0's upkeep so a full turn's worth of priority windows is ahead.
    fn spell_less_state() -> GameState {
        let mut state = GameState::new_two_player();
        state.step = Step::Upkeep;
        for seat in 0..2 {
            let lib: Vec<_> = (0..12)
                .map(|_| state.new_instance(fixture("onakke_ogre")))
                .collect();
            state.players[seat].library = lib;
        }
        state
    }

    /// Choose this seat's move from a view: pass if offered, else the sole forced
    /// choice (an empty combat declaration), with no targets. A minimal inline driver
    /// (no rule-based agent, to avoid a crate cycle) that stands in for a human's clicks.
    fn forced_move(view: &GameView) -> ChooseAction {
        let action = view
            .valid_actions
            .iter()
            .find(|a| a.kind == "pass_priority")
            .or_else(|| view.valid_actions.iter().find(|a| a.kind != "concede"))
            .expect("an actionable view offers a move");
        ChooseAction {
            action_id: action.id.clone(),
            token: action.token.clone(),
            targets: Vec::new(),
        }
    }

    /// Drive the room with [`forced_move`] until either seat's latest view reaches
    /// `until_turn`, returning how many messages the driver had to send — a proxy for
    /// how many manual clicks the turn cost.
    async fn count_clicks_until_turn(
        handle: &RoomHandle,
        rx0: &mut watch::Receiver<Option<GameView>>,
        rx1: &mut watch::Receiver<Option<GameView>>,
        until_turn: u32,
    ) -> usize {
        let mut clicks = 0usize;
        for _ in 0..1000usize {
            let v0 = rx0.borrow_and_update().clone();
            let v1 = rx1.borrow_and_update().clone();
            if v0
                .as_ref()
                .or(v1.as_ref())
                .is_some_and(|v| v.turn >= until_turn)
            {
                return clicks;
            }
            let actor = if v0.as_ref().is_some_and(|v| !v.valid_actions.is_empty()) {
                v0.map(|v| (0usize, v))
            } else if v1.as_ref().is_some_and(|v| !v.valid_actions.is_empty()) {
                v1.map(|v| (1usize, v))
            } else {
                None
            };
            match actor {
                Some((seat, view)) => {
                    clicks += 1;
                    handle.send(RoomInput::Message {
                        seat,
                        message: ClientMessage::ChooseAction(forced_move(&view)),
                    });
                    tokio::select! {
                        _ = rx0.changed() => {}
                        _ = rx1.changed() => {}
                    }
                }
                None => {
                    tokio::select! {
                        r0 = rx0.changed() => { if r0.is_err() { break; } }
                        r1 = rx1.changed() => { if r1.is_err() { break; } }
                    }
                }
            }
        }
        clicks
    }

    #[tokio::test]
    async fn issue_264_automation_off_by_default_elides_stops_and_indicator() {
        // The default policy changes nothing on the wire: no stops, never auto-passed.
        let (handle, task) = Room::new(dealt_state(), db()).spawn();
        let (tx0, mut rx0) = view_channel();
        handle.send(RoomInput::Join {
            seat: 0,
            outbox: tx0,
        });
        let view0 = wait_for_view(&mut rx0).await;
        assert!(view0.stops.is_empty(), "no stops by default");
        assert!(
            !view0.auto_passed,
            "nothing is auto-passed under the off policy"
        );
        assert!(
            view0
                .valid_actions
                .iter()
                .any(|a| a.kind == "pass_priority"),
            "the seat still gets a manual pass with automation off"
        );
        drop(handle);
        task.await.unwrap();
    }

    #[tokio::test]
    async fn issue_264_auto_pass_dramatically_reduces_manual_passes_on_a_spell_less_turn() {
        // Acceptance: with default stops, a spell-less turn requires dramatically fewer
        // manual passes. Drive the identical spell-less turn twice — automation off vs
        // on — and count the clicks each cost.
        let (off_handle, off_task) = Room::new(spell_less_state(), db()).spawn();
        let (tx0, mut off0) = view_channel();
        let (tx1, mut off1) = view_channel();
        off_handle.send(RoomInput::Join {
            seat: 0,
            outbox: tx0,
        });
        off_handle.send(RoomInput::Join {
            seat: 1,
            outbox: tx1,
        });
        let off_clicks = count_clicks_until_turn(&off_handle, &mut off0, &mut off1, 2).await;
        drop(off_handle);
        off_task.await.unwrap();

        let (on_handle, on_task) = Room::new(spell_less_state(), db())
            .with_auto_pass(AutoPassPolicy::On)
            .spawn();
        let (tx0, mut on0) = view_channel();
        let (tx1, mut on1) = view_channel();
        on_handle.send(RoomInput::Join {
            seat: 0,
            outbox: tx0,
        });
        on_handle.send(RoomInput::Join {
            seat: 1,
            outbox: tx1,
        });
        let on_clicks = count_clicks_until_turn(&on_handle, &mut on0, &mut on1, 2).await;
        drop(on_handle);
        on_task.await.unwrap();

        assert!(
            off_clicks >= 8,
            "the manual baseline spends many passes on a spell-less turn: {off_clicks}"
        );
        assert!(
            on_clicks * 3 < off_clicks,
            "automation makes a spell-less turn dramatically cheaper: on={on_clicks} off={off_clicks}"
        );
    }

    #[tokio::test]
    async fn issue_264_stop_preferences_survive_reconnect() {
        // Preferences set over the wire are held on the room, so a disconnect/reconnect
        // re-sends them in full — they never live only in client memory.
        let (handle, task) = Room::new(dealt_state(), db()).spawn();
        let (tx0, mut rx0) = view_channel();
        handle.send(RoomInput::Join {
            seat: 0,
            outbox: tx0,
        });
        let _ = wait_for_view(&mut rx0).await;

        handle.send(RoomInput::Message {
            seat: 0,
            message: ClientMessage::SetStops(SetStops {
                stops: vec![Phase::Upkeep, Phase::End],
            }),
        });
        let after = wait_for_view(&mut rx0).await;
        assert_eq!(after.stops, vec![Phase::Upkeep, Phase::End]);

        // Disconnect and reconnect with a fresh outbox: the stops come back in full.
        handle.send(RoomInput::Leave { seat: 0 });
        let (tx0b, mut rx0b) = view_channel();
        handle.send(RoomInput::Join {
            seat: 0,
            outbox: tx0b,
        });
        let resumed = wait_for_view(&mut rx0b).await;
        assert_eq!(
            resumed.stops,
            vec![Phase::Upkeep, Phase::End],
            "stop preferences survive reconnect"
        );

        drop(handle);
        task.await.unwrap();
    }

    #[tokio::test]
    async fn issue_264_a_relevant_stop_keeps_priority_at_an_idle_step() {
        // A seat that has opted to stop at a step still receives priority there even
        // when idle — the escape hatch from an auto-pass chain. Seat 0 is idle at its
        // postcombat main; with a stop there it is handed priority rather than passed.
        let mut state = spell_less_state();
        state.step = Step::PostcombatMain;
        let (handle, task) = Room::new(state, db())
            .with_auto_pass(AutoPassPolicy::On)
            .with_stops(vec![vec![Phase::PostcombatMain], vec![]])
            .spawn();
        let (tx0, mut rx0) = view_channel();
        handle.send(RoomInput::Join {
            seat: 0,
            outbox: tx0,
        });
        let view0 = wait_for_view(&mut rx0).await;
        assert_eq!(
            view0.phase,
            Phase::PostcombatMain,
            "the stop halts the settle at the postcombat main"
        );
        assert!(
            view0
                .valid_actions
                .iter()
                .any(|a| a.kind == "pass_priority"),
            "the stopped seat is handed priority (a manual pass), not auto-passed"
        );
        drop(handle);
        task.await.unwrap();
    }

    #[tokio::test]
    async fn issue_264_without_a_stop_the_same_idle_step_is_auto_passed() {
        // The control for the test above: the same idle postcombat main, no stop, is
        // auto-passed through — the seat never rests there.
        let mut state = spell_less_state();
        state.step = Step::PostcombatMain;
        let (handle, task) = Room::new(state, db())
            .with_auto_pass(AutoPassPolicy::On)
            .spawn();
        let (tx0, mut rx0) = view_channel();
        handle.send(RoomInput::Join {
            seat: 0,
            outbox: tx0,
        });
        // The settle fast-forwards past the postcombat main to the next forced choice
        // (a combat declaration on the following turn); seat 0's resting view is no
        // longer a pass at its postcombat main.
        let view0 = wait_for_view(&mut rx0).await;
        assert!(
            !(view0.phase == Phase::PostcombatMain
                && view0.turn == 1
                && view0
                    .valid_actions
                    .iter()
                    .any(|a| a.kind == "pass_priority")),
            "with no stop the idle postcombat main is auto-passed, not rested on"
        );
        drop(handle);
        task.await.unwrap();
    }

    #[tokio::test]
    async fn issue_264_a_castable_instant_is_never_auto_passed() {
        // Safety: a seat with an instant-speed play always keeps priority, even with
        // automation on and no stop. Seat 1 holds an affordable instant on seat 0's
        // turn; the engine reports it non-idle, so the room never passes for it.
        let mut state = GameState::new_two_player();
        state.step = Step::Upkeep; // seat 0's turn, seat 1 may respond at instant speed
        let bolt = state.new_instance(fixture("cancel"));
        state.players[1].hand = vec![bolt];
        // `cancel` costs {1}{U}{U}; three blue pays both blue pips and the generic.
        state.players[1].mana_pool.add(rune_engine::Color::Blue, 3);
        // Something on the stack for the counterspell to legally target.
        let boar = state.new_instance(fixture("onakke_ogre"));
        let sid = rune_engine::StackId(state.mint_id());
        state.stack.push(rune_engine::StackObject {
            id: sid,
            controller: PlayerId(0),
            kind: rune_engine::StackObjectKind::Spell { card: boar },
            targets: Vec::new(),
        });
        state.priority = PlayerId(1);

        let (handle, task) = Room::new(state, db())
            .with_auto_pass(AutoPassPolicy::On)
            .spawn();
        let (tx1, mut rx1) = view_channel();
        handle.send(RoomInput::Join {
            seat: 1,
            outbox: tx1,
        });
        let view1 = wait_for_view(&mut rx1).await;
        assert!(
            view1.valid_actions.iter().any(|a| a.kind == "cast_spell"),
            "a seat with a castable instant keeps priority — never auto-passed out of a response"
        );
        assert!(!view1.auto_passed, "the seat was not auto-passed");
        drop(handle);
        task.await.unwrap();
    }

    #[tokio::test]
    async fn issue_264_auto_passed_indicator_flags_the_skipped_seat() {
        // The display-only indicator: reaching the first forced decision auto-passes
        // seat 0 through the early idle steps, so its resting view is flagged.
        let (handle, task) = Room::new(spell_less_state(), db())
            .with_auto_pass(AutoPassPolicy::On)
            .spawn();
        let (tx0, mut rx0) = view_channel();
        handle.send(RoomInput::Join {
            seat: 0,
            outbox: tx0,
        });
        let view0 = wait_for_view(&mut rx0).await;
        assert!(
            view0.auto_passed,
            "seat 0 was auto-passed to reach its first forced decision (indicator set)"
        );
        // It rests on the forced attacker declaration (no pass on offer there).
        assert!(
            view0
                .valid_actions
                .iter()
                .any(|a| a.kind == "declare_attackers"),
            "the settle halted at the active player's forced combat declaration"
        );
        drop(handle);
        task.await.unwrap();
    }
}
