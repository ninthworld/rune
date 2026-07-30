//! Event logging for the game state.

use super::{GameEvent, GameLogEntry, GameState};

impl GameState {
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
