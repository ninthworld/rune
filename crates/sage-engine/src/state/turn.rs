//! Turn structure and advancement methods.

use crate::id::PlayerId;
use crate::mana::ManaPool;
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
    ///
    /// It *does* empty every player's mana pool (CR 500.4: "When a step or phase
    /// ends, any unused mana left in a player's mana pool empties"). That belongs
    /// here rather than in the pipeline wrapper because this method is the single
    /// point through which a step or phase ever ends: the natural [`Step::next`]
    /// walk, a queued extra step, and the turn boundary via
    /// [`Self::begin_next_turn`] all funnel through it, and the pipeline's walk
    /// past the no-priority steps calls it once per step it crosses. Emptying here
    /// makes CR 500.4 a property of the state machine rather than something every
    /// caller must remember.
    #[must_use]
    pub fn advance(&self) -> Self {
        let mut next = self.clone();
        // CR 500.4: the ending step empties every pool — the active player's and
        // everyone else's — before the next step is entered, so no turn-based
        // action of the incoming step ever observes stale floating mana.
        next.empty_all_mana_pools();
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
    fn begin_next_turn(&mut self) {
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
        // CR 302.6 reads from "the beginning of its controller's most recent turn",
        // which is now, for whoever just took the turn. Recorded here — the single
        // point through which a turn ever begins — so extra turns and skipped
        // eliminated seats are accounted for by construction.
        if let Some(player) = self.players.get_mut(self.active_player.0) {
            player.turn_began = self.turn;
        }
        self.step = Step::Untap;
        self.land_played = false;
        // A new turn is a new combat: the previous turn's declarations no longer
        // apply (CR 508.1 / 509.1 are performed afresh each combat).
        self.attackers_declared = false;
        self.blockers_declared = false;
        self.damage_orders.clear();
        self.blockers_declared_by.clear();
        // CR 606.3: the one-loyalty-ability-per-turn allowance refreshes with the turn.
        self.loyalty_activations.clear();
        // A "cast from your graveyard **this turn**" permission lapses with the turn it
        // was granted on. Cleared here rather than compared everywhere, so a stale entry
        // cannot outlive its turn even by one read.
        self.graveyard_casting.clear();
        self.exile_playing.clear();
        // The same for a "this turn" permission to ignore hexproof: one boundary drops
        // every per-turn permission, so neither can outlive its turn.
        self.ignoring_hexproof.clear();
        // And the same for a one-shot replacement effect created this turn (CR 614.1b):
        // `the next time … this turn` lapses unused if the event never came. Clearing it
        // here rather than comparing the turn at every read is what stops a stale
        // replacement from modifying an event a turn later.
        self.replacements.clear();
        // And the same again for a delayed trigger whose `this turn` has run out
        // (CR 603.7b): `when you next cast a spell **this turn**` lapses unfired if the
        // spell was never cast, and one boundary drops every per-turn record.
        self.delayed_triggers.clear();
    }

    /// Empty every player's mana pool on this owned state (CR 500.4). Applies to
    /// all seats, not just the active player: an opponent who floated mana in
    /// response loses it at the same step boundary. Mana burn was removed from the
    /// rules, so unspent mana simply vanishes with no penalty.
    fn empty_all_mana_pools(&mut self) {
        for player in &mut self.players {
            player.mana_pool = ManaPool::default();
        }
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

#[cfg(test)]
impl GameState {
    /// Walk [`Self::advance`] until the turn number changes, returning the state at
    /// the untap step of the next turn.
    ///
    /// Test-only, and deliberately not a plain `turn += 1`: seat rotation, extra
    /// turns, skipped eliminated seats, and [`crate::player::Player::turn_began`]
    /// are all produced by the FSM, so a test that reaches turn *n* this way is
    /// asserting against a turn the engine agrees exists rather than one the test
    /// declared into being. Any rule measured from "its controller's most recent
    /// turn" (CR 302.6) is only honestly testable this way.
    #[must_use]
    pub(crate) fn advance_to_next_turn(&self) -> Self {
        let start = self.turn;
        let mut next = self.clone();
        while next.turn == start {
            next = next.advance();
        }
        next
    }
}
