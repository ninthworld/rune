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
//! - [`policy`] — the [`TimerPolicy`]/[`AutoPassPolicy`] policies and the timeout
//!   default action.
//! - [`driver`] — [`Room::spawn`], the [`Room::run`] loop, and the decision clock.
//! - [`input`] — client-message routing and the auto-pass settle loop.
//! - [`broadcast`] — seat/spectator plumbing and personalized-view fan-out.
//! - [`connection`] — the WebSocket bridges [`serve_connection`]/[`serve_spectator_connection`].
//!
//! The [`Room`] struct and its constructors live here so every submodule's
//! `impl Room` block can reach the private fields as an ancestor module.

use rune_engine::{CardDatabase, GameState};
use rune_protocol::{GameView, Phase, SpectatorView};
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
pub use policy::AutoPassPolicy;
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
}
