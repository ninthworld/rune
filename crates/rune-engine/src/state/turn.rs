//! Turn structure and advancement methods.

use crate::id::PlayerId;
use crate::phase::Step;

use super::GameState;

impl GameState {
    /// Advance the game to the next step of the turn structure, returning a new
    /// state (the input is never mutated).
    ///
    /// Order of precedence: a queued [extra step](Self::extra_steps) is visited
    /// first; otherwise the turn walks its natural sequence via [`Step::next`];
    /// advancing past [`Step::Cleanup`] ends the turn and begins the next one.
    ///
    /// This is the turn-structure FSM only. It does not touch priority, the
    /// stack, or state-based actions — those arrive with the action pipeline.
    #[must_use]
    pub fn advance(&self) -> Self {
        let mut next = self.clone();
        if let Some(step) = next.extra_steps.pop() {
            next.step = step;
        } else if next.step == Step::Cleanup {
            next.begin_next_turn();
        } else {
            next.step = next.step.next();
        }
        next
    }

    /// Begin the next turn on this owned state: bump the turn counter, hand the
    /// turn to the taker of a pending [extra turn](Self::extra_turns) or, absent
    /// one, to the next player in seating order, and reset to [`Step::Untap`].
    ///
    /// A no-op on a seatless state, so player rotation never divides by zero.
    pub(crate) fn begin_next_turn(&mut self) {
        if self.players.is_empty() {
            return;
        }
        self.turn += 1;
        self.active_player = loop {
            match self.extra_turns.pop() {
                // CR 800.4a: an extra turn owed to an eliminated player is discarded,
                // and the search continues for the real next turn.
                Some(taker) if self.players.get(taker.0).is_some_and(|p| p.has_lost) => continue,
                Some(taker) => break taker,
                // No extra turn owed to a living player: the next seat still in the
                // game takes the turn, skipping every eliminated seat (CR 800.4a).
                None => {
                    break self
                        .next_living_seat(self.active_player)
                        .unwrap_or(self.active_player)
                }
            }
        };
        self.step = Step::Untap;
        self.land_played = false;
        // A new turn is a new combat: the previous turn's declarations no longer
        // apply (CR 508.1 / 509.1 are performed afresh each combat).
        self.attackers_declared = false;
        self.blockers_declared = false;
        self.damage_orders.clear();
        self.blockers_declared_by.clear();
    }

    /// Return a copy with an extra turn granted to `player`. Because extra turns
    /// are taken LIFO, this turn is taken before any extra turn granted earlier
    /// (MTG rule 720.1).
    #[must_use]
    pub fn with_extra_turn(&self, player: PlayerId) -> Self {
        let mut next = self.clone();
        next.extra_turns.push(player);
        next
    }

    /// Return a copy with `step` queued as an extra step, visited before the
    /// turn's natural sequence resumes. Queue the steps of an additional phase
    /// in reverse so they are visited in play order.
    #[must_use]
    pub fn with_extra_step(&self, step: Step) -> Self {
        let mut next = self.clone();
        next.extra_steps.push(step);
        next
    }
}
