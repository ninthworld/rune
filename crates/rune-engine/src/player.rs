//! Per-player state and its private zones.

use crate::id::CardId;
use crate::mana::ManaPool;
use crate::zone::Zone;

/// Life total every player starts a game with.
pub const STARTING_LIFE: i32 = 20;

/// A single player's state: their life total and the four zones they own.
///
/// Cards are stored as ordered piles of [`CardId`]; the top of the library is
/// the last element. The shared battlefield lives on [`crate::GameState`], not
/// here.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Player {
    /// Current life total. May be negative before state-based actions resolve.
    pub life: i32,
    /// Whether this player has lost the game. Set by the state-based-actions
    /// loop (e.g. on reaching 0 or less life); never unset.
    pub has_lost: bool,
    /// The player's deck (private, ordered).
    pub library: Vec<CardId>,
    /// Cards in the player's hand (private).
    pub hand: Vec<CardId>,
    /// The player's graveyard (public, ordered).
    pub graveyard: Vec<CardId>,
    /// Cards this player owns in exile.
    pub exile: Vec<CardId>,
    /// Unspent mana in the player's pool. Emptied between steps (not yet modeled
    /// for the vertical slice, which spends mana within one step).
    pub mana_pool: ManaPool,
}

impl Player {
    /// A fresh player at [`STARTING_LIFE`] with empty zones.
    #[must_use]
    pub fn new() -> Self {
        Self {
            life: STARTING_LIFE,
            ..Self::default()
        }
    }

    /// Borrow one of the player's private zones by name.
    #[must_use]
    pub fn zone(&self, zone: Zone) -> &Vec<CardId> {
        match zone {
            Zone::Library => &self.library,
            Zone::Hand => &self.hand,
            Zone::Graveyard => &self.graveyard,
            Zone::Exile => &self.exile,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn player_zone_accessor_matches_fields() {
        let mut player = Player::new();
        player.hand.push(CardId(7));
        player.graveyard.push(CardId(9));
        assert_eq!(player.zone(Zone::Hand), &vec![CardId(7)]);
        assert_eq!(player.zone(Zone::Graveyard), &vec![CardId(9)]);
        assert!(player.zone(Zone::Library).is_empty());
        assert!(player.zone(Zone::Exile).is_empty());
    }
}
