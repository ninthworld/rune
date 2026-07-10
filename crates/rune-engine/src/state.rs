//! The root game state and the shared battlefield.

use crate::id::{CardId, PermanentId, PlayerId};
use crate::phase::Step;
use crate::player::Player;

/// A permanent on the shared battlefield.
///
/// Its [`PermanentId`] is minted fresh on battlefield entry and is distinct
/// from the [`CardId`] of the card it represents.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Permanent {
    /// Battlefield identity, fresh on entry.
    pub id: PermanentId,
    /// The card this permanent represents.
    pub card: CardId,
    /// The player who currently controls it.
    pub controller: PlayerId,
    /// Whether the permanent is tapped.
    pub tapped: bool,
}

/// The complete, immutable state of a game at one moment.
///
/// Every field is either raw state or a stable id; nothing derivable (current
/// characteristics, legal actions, whose turn it "feels" like) is stored here —
/// those are computed on demand by pure functions.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct GameState {
    /// Current turn number (1-based); `0` in the empty [`Default`] state.
    pub turn: u32,
    /// The player whose turn it is, as an index into [`Self::players`].
    pub active_player: PlayerId,
    /// The current phase/step of the turn.
    pub step: Step,
    /// Every player, in seating (turn) order.
    pub players: Vec<Player>,
    /// The shared battlefield, owned by the game rather than any one player.
    pub battlefield: Vec<Permanent>,
    /// Extra turns waiting to be taken, as a stack: the entry pushed last is
    /// taken first (MTG rule 720.1 — the most recently created extra turn
    /// happens first). Each entry is the player who takes that turn.
    pub extra_turns: Vec<PlayerId>,
    /// Extra steps to visit before the turn's natural sequence resumes, as a
    /// stack: the entry pushed last is visited first. An additional phase
    /// (e.g. an extra combat) is represented by queueing its constituent steps.
    pub extra_steps: Vec<Step>,
}

impl GameState {
    /// An initial two-player game: turn 1, player 0 to act, at the [`Step::Untap`]
    /// step of the first turn. Both players start with empty libraries — deck
    /// loading arrives with the card database (issue #9).
    #[must_use]
    pub fn new_two_player() -> Self {
        Self {
            turn: 1,
            active_player: PlayerId(0),
            step: Step::Untap,
            players: vec![Player::new(), Player::new()],
            battlefield: Vec::new(),
            extra_turns: Vec::new(),
            extra_steps: Vec::new(),
        }
    }

    /// Borrow the active player, or `None` if [`Self::active_player`] is out of
    /// range (as it is in the empty [`Default`] state).
    #[must_use]
    pub fn active_player(&self) -> Option<&Player> {
        self.players.get(self.active_player.0)
    }

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
    fn begin_next_turn(&mut self) {
        if self.players.is_empty() {
            return;
        }
        self.turn += 1;
        self.active_player = match self.extra_turns.pop() {
            Some(taker) => taker,
            None => PlayerId((self.active_player.0 + 1) % self.players.len()),
        };
        self.step = Step::Untap;
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
