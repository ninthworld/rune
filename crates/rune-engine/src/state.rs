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
        }
    }

    /// Borrow the active player, or `None` if [`Self::active_player`] is out of
    /// range (as it is in the empty [`Default`] state).
    #[must_use]
    pub fn active_player(&self) -> Option<&Player> {
        self.players.get(self.active_player.0)
    }
}
