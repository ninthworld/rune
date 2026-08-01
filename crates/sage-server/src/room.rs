//! The room task — layer 2 of `docs/brief.md`.
//!
//! A room is one async task that owns exactly one [`sage_engine`] game and is the
//! sole writer of its state. Connections never touch the game directly; they send
//! [`RoomInput`] messages over the room's channel, and the room applies chosen
//! actions through the engine and pushes each connected seat its own personalized
//! [`GameView`]. Because the only mutable state lives inside the task, no two rooms
//! ever share game state.
//!
//! The room contains **no game logic**: it routes an `action_id` back to the
//! engine's own [`valid_actions`](sage_engine::valid_actions)/[`apply_action`](sage_engine::apply_action)
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
//! - **Room inbox** is a bounded [`mpsc`](tokio::sync::mpsc) channel
//!   ([`ROOM_INBOX_CAPACITY`]). Inputs are delivered with `try_send`; once the queue
//!   is full, further inputs from a flooding client are **dropped** (logged) rather
//!   than buffered. The room stays alive and keeps serving; a client can only ever
//!   hurt its own throughput, never the server's memory.
//!
//! # Module layout (issue #427)
//! This module is a barrel over cohesive submodules, split from a single oversized
//! file by pure code motion:
//! - [`handle`] — the [`Seat`] alias, [`RoomInput`] messages, and [`RoomHandle`].
//! - [`policy`] — the [`TimerPolicy`]/[`AutoPassPolicy`]/[`StopPolicy`] policies,
//!   the per-seat stop preference, and the timeout default action.
//! - [`driver`] — [`Room::spawn`], the [`Room::run`] loop, and the decision clock.
//! - [`input`] — client-message routing and the auto-pass settle loop.
//! - [`broadcast`] — seat/spectator plumbing and personalized-view fan-out.
//! - [`connection`] — the WebSocket bridges [`serve_connection`]/[`serve_spectator_connection`].
//!
//! The [`Room`] struct and its constructors live here so every submodule's
//! `impl Room` block can reach the private fields as an ancestor module.

use sage_engine::{CardDatabase, GameState};
use sage_protocol::{ActionAck, AutoPassedStep, GameView, MatchFormat, Phase, SpectatorView};
use tokio::sync::watch;
use tokio::time::Instant;

mod broadcast;
mod connection;
mod driver;
mod handle;
mod input;
mod policy;
#[cfg(test)]
mod test_support;

pub use connection::{serve_connection, serve_spectator_connection};
pub use handle::{RoomHandle, RoomInput, Seat};
use policy::SeatStops;
pub use policy::{AutoPassPolicy, StopPolicy};
// `TimerPolicy` is reachable only through `Room::with_timer_policy` (the lobby never
// re-exports it), so the barrel re-export stays crate-internal — the same reach it
// had when the enum was defined inline in this module.
pub(crate) use policy::TimerPolicy;

/// Bound on the room's input queue. Inputs beyond this depth from a flooding client
/// are dropped (see [`RoomHandle::send`]); the value is generous enough that a
/// well-behaved pair of clients never approaches it, yet fixed so a misbehaving peer
/// cannot grow server memory.
const ROOM_INBOX_CAPACITY: usize = 1024;

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
/// deciding player's behalf ([`timeout_default_action`](crate::room::policy::timeout_default_action));
/// a single missed prompt never concedes.
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
    /// [`GameView::player_names`] rather than the pure
    /// [`personalized_view`](crate::view::personalized_view) shim.
    /// Empty (all-`None`) when no name was ever set, so the map elides from the wire.
    player_names: Vec<Option<String>>,
    /// Which seats are played by a server-side **AI** (issue #415/#553), in seat
    /// order. Public presentation information the lobby already shows before the
    /// game; carried here — like [`Self::player_names`], a *session* concern the
    /// engine knows nothing about — so every seat's view can mark the seat instead
    /// of losing the fact at the hand-off. All-`false` (and empty) by default.
    ai_seats: Vec<bool>,
    /// The **format** this room's game is played under (issue #553), or `None` when
    /// the room was constructed without one (unit tests, and any older path). Room
    /// state by definition: the format registry lives in the server, the engine holds
    /// no format policy at all. Projected into every seat's
    /// [`GameView::format`] and every spectator's, so a client can render
    /// Commander-specific presentation without inferring it from zone contents.
    format: Option<MatchFormat>,
    /// The basic priority-automation policy (issue #264). [`AutoPassPolicy::Off`] by
    /// default, so automation is opt-in and existing behavior is unchanged.
    auto_pass: AutoPassPolicy,
    /// Each seat's priority-stop preferences in seat order (issue #264, ADR 0010):
    /// the steps at which that seat wants priority even when the engine reports no
    /// meaningful action — or no *choice* in a declaration it owes (issue #453) — so
    /// automation does not settle past it there. Set over the protocol
    /// (`set_stops`) and held here — like [`Self::player_names`], a per-seat concern
    /// that is *not* engine state — so a preference survives a disconnect/reconnect
    /// (the room is never torn down on leave). Reflected back in each seat's
    /// [`GameView::stops`]/[`GameView::own_turn_stops`].
    ///
    /// `None` means **this seat has never expressed a preference**, so
    /// [`Self::stop_policy`] seeds one (issue #455). That distinction is the whole
    /// point of the `Option`: "never asked" and "explicitly asked for nothing" have
    /// to differ, or a player could not clear a default stop — they would send an
    /// empty set and the room would hand the default straight back.
    stops: Vec<Option<SeatStops>>,
    /// The **default-stop** policy (issue #455) that seeds a seat which has never
    /// sent `set_stops`. [`StopPolicy::None`] by default, so every existing room —
    /// and every headless or AI-only game — starts exactly where ADR 0010 left it.
    stop_policy: StopPolicy,
    /// The turn-and-step positions the room acted at on each seat's behalf during the
    /// most recent settle (issues #264 and #455), in the order it acted: a transient,
    /// display-only signal, recomputed each settle and projected into that seat's
    /// [`GameView::auto_passed`]/[`GameView::auto_passed_steps`] on the following
    /// broadcast so a client can say not just *that* it was skipped but *where* — and,
    /// because each entry carries its own turn, where a boundary actually fell.
    /// Not load-bearing state — the authoritative record of a settle is the game log.
    auto_passed_steps: Vec<Vec<AutoPassedStep>>,
    /// Where each seat's most recent unattended stretch began in the game log, as a
    /// log `sequence` — the companion to [`Self::auto_passed_steps`] that says *what*
    /// happened rather than only *where* (issue #644).
    ///
    /// Captured **before the action that triggered the settle** rather than at the
    /// settle's first pass. An opponent's spell is logged by their own action, and a
    /// report that began after it would omit the event a player is trying to
    /// understand — which is the whole reason this exists.
    ///
    /// Transient and display-only in exactly the way the steps beside it are:
    /// recomputed on every settle, projected onto the next broadcast, and never read
    /// by the game.
    auto_passed_from: Vec<Option<u64>>,
    /// The **acknowledgement** each seat is owed for its most recent correlated
    /// submission (issue #554), indexed by seat. Written when a `ChooseAction`
    /// carrying a [`ChooseAction::submission`](sage_protocol::ChooseAction) is routed
    /// — accepted or rejected — and **taken** by the next view sent to that seat, so
    /// it is delivered exactly once and a later resync never re-fires it. Transient
    /// and display-only, like [`Self::auto_passed_seats`]; the game never reads it.
    pending_acks: Vec<Option<ActionAck>>,
    /// The connected **spectators** (issue #351): each a latest-value sender
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
            ai_seats: Vec::new(),
            format: None,
            auto_pass: AutoPassPolicy::Off,
            stops: vec![None; seat_count],
            stop_policy: StopPolicy::None,
            auto_passed_steps: vec![Vec::new(); seat_count],
            auto_passed_from: vec![None; seat_count],
            pending_acks: (0..seat_count).map(|_| None).collect(),
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

    /// Set this room's **default-stop** policy (issue #455). Chainable on
    /// [`Room::new`]; the default is [`StopPolicy::None`], which reproduces exactly
    /// ADR 0010's "stop nowhere" starting preference. The policy only seeds a seat
    /// that has never sent `set_stops`; the first one it sends replaces the seed.
    #[must_use]
    pub fn with_stop_policy(mut self, policy: StopPolicy) -> Self {
        self.stop_policy = policy;
        self
    }

    /// Preset each seat's **any-turn** priority-stop preferences (issue #264),
    /// indexed by seat. Chainable on [`Room::new`]; the default is no stops for any
    /// seat. A seat with an index past the end of `stops` keeps its default. In
    /// production the preferences arrive over the wire (`set_stops`); this seeds
    /// them for tests.
    ///
    /// Presetting a seat counts as that seat having expressed a preference, so it
    /// also opts that seat out of any [`StopPolicy`] seed — including an entry that
    /// is deliberately empty.
    #[must_use]
    pub fn with_stops(mut self, stops: Vec<Vec<Phase>>) -> Self {
        for (seat, set) in stops.into_iter().enumerate() {
            if let Some(slot) = self.stops.get_mut(seat) {
                slot.get_or_insert_with(SeatStops::default).any_turn = set;
            }
        }
        self
    }

    /// Preset each seat's **own-turn** priority-stop preferences (issue #455),
    /// indexed by seat: the steps that seat wants priority at while it is the active
    /// player. The sibling of [`Room::with_stops`], and it opts a seat out of the
    /// [`StopPolicy`] seed in exactly the same way.
    #[must_use]
    pub fn with_own_turn_stops(mut self, stops: Vec<Vec<Phase>>) -> Self {
        for (seat, set) in stops.into_iter().enumerate() {
            if let Some(slot) = self.stops.get_mut(seat) {
                slot.get_or_insert_with(SeatStops::default).own_turn = set;
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

    /// Mark which seats are played by a server-side **AI** (issue #553), indexed by
    /// seat. Chainable on [`Room::new`]; the default is every seat human, so
    /// `OpponentView::ai`/`SelfView::ai` stay `false` and elide from the wire. A seat
    /// index past the end of `ai` is treated as human. Mirrors
    /// [`Room::with_player_names`]: the same lobby knowledge, carried the same way.
    #[must_use]
    pub fn with_ai_seats(mut self, ai: Vec<bool>) -> Self {
        self.ai_seats = ai;
        self
    }

    /// Set the **format** this room's game is played under (issue #553). Chainable on
    /// [`Room::new`]; the default is `None` (unknown format, not Commander), which is
    /// exactly what an older server's views said. Mirrors
    /// [`Room::with_player_names`]: format is registry/lobby knowledge, never engine
    /// state, so it is carried on the room and projected after the pure shim.
    #[must_use]
    pub fn with_format(mut self, format: MatchFormat) -> Self {
        self.format = Some(format);
        self
    }
}
