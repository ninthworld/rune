//! The room's input channel surface: the [`Seat`] alias, the [`RoomInput`]
//! messages every game mutation originates from, and the cloneable [`RoomHandle`]
//! that delivers them over the bounded inbox. Pure code motion out of the room
//! module root (issue #427) — no behavior change.

use rune_protocol::ClientMessage;
use tokio::sync::mpsc;
use tracing::warn;

use super::*;

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
    /// Bounded sender into the room task's inbox. `pub(super)` so the room task
    /// ([`Room::spawn`]) — which lives in a sibling submodule — can mint a handle
    /// around the channel it owns; unchanged in behavior from when it was private
    /// within a single-file module.
    pub(super) inbox: mpsc::Sender<RoomInput>,
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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

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
