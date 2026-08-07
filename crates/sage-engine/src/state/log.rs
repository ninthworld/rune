//! Event logging for the game state.

use super::{GameEvent, GameLogEntry, GameState};
use crate::id::PlayerId;

impl GameState {
    /// This state, plus a log entry saying the table was rolled back to it at
    /// `player`'s request (issue #648).
    ///
    /// The server owns undo — the checkpoints, the limit, and the decision to restore
    /// one are all its (ADR 0010's seam). What it cannot own is the log: the window and
    /// its sequence numbers are engine state, and a caller that appended to them from
    /// outside would be minting history the engine did not number. So the room hands
    /// the restored state back through here and the engine numbers the entry, exactly
    /// as it numbers every other event.
    ///
    /// Pure, like everything else in this crate: the restored state is returned, never
    /// mutated in place, and nothing about the rollback is remembered beyond the entry.
    #[must_use]
    pub fn with_undo_recorded(&self, player: PlayerId) -> Self {
        let mut next = self.clone();
        next.record_event(GameEvent::Undone { player });
        next
    }

    /// Append an event to the authoritative recent-history window.
    pub(crate) fn record_event(&mut self, event: GameEvent) {
        const LOG_WINDOW: usize = 200;
        self.log.push(GameLogEntry {
            sequence: self.next_log_sequence,
            event,
        });
        self.next_log_sequence += 1;
        if self.log.len() > LOG_WINDOW {
            self.log.remove(0);
        }
    }
}
