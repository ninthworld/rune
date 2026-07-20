//! Per-player state and its private zones.

use crate::commander::CommanderState;
use crate::id::CardInstance;
use crate::mana::ManaPool;
use crate::zone::Zone;

/// Life total every player starts a game with.
pub const STARTING_LIFE: i32 = 20;

/// Default maximum hand size (CR 402.2). At the cleanup step a player with more
/// than this many cards discards down to it as a turn-based action (CR 514.1).
pub const MAX_HAND_SIZE: usize = 7;

/// Why a player lost the game — the unified set of losing conditions the engine
/// models (CR 104.3 / CR 704.5). Recorded on the losing [`Player`] when the loss
/// is registered; the terminal [`GameResult`](crate::GameResult) surfaces the
/// deciding one on the wire.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LossReason {
    /// CR 704.5a — the player was at 0 or less life when state-based actions were
    /// checked.
    ZeroLife,
    /// CR 704.5c — the player attempted to draw a card from an empty library.
    DrewFromEmptyLibrary,
    /// CR 104.3a — the player conceded, leaving the game.
    Concede,
    /// CR 903.10a — the player was dealt 21 or more combat damage over the game
    /// by a single commander (see
    /// [`GameState::commander_damage`](crate::GameState::commander_damage) and
    /// [`COMMANDER_DAMAGE_LOSS_THRESHOLD`](crate::commander::COMMANDER_DAMAGE_LOSS_THRESHOLD)).
    CommanderDamage,
}

/// A single player's state: their life total and the zones they own.
///
/// The four private zones (library, hand, graveyard, exile) plus the public
/// command zone (CR 408, holding a designated commander). Cards are stored as
/// ordered piles of [`CardInstance`]s, so two copies of the same printing stay
/// individually addressable; the top of the library is the last element. The
/// shared battlefield lives on [`crate::GameState`], not here. A player's
/// commander bookkeeping (tax, pending return) rides in
/// [`commander`](Self::commander); see [`crate::commander`].
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Player {
    /// Current life total. May be negative before state-based actions resolve.
    pub life: i32,
    /// Whether this player has lost the game. Set by the state-based-actions
    /// loop (e.g. on reaching 0 or less life) or by conceding; never unset.
    pub has_lost: bool,
    /// Why this player lost, set alongside [`Self::has_lost`] and never unset.
    /// `None` while the player is still in the game. Used to surface the deciding
    /// reason in the terminal [`GameResult`](crate::GameResult).
    pub loss_reason: Option<LossReason>,
    /// Whether this player has *left the game* under CR 800.4a — their objects have
    /// been removed and the elimination logged. Distinct from [`Self::has_lost`]:
    /// in a two-player game a loss ends the game (CR 104.2a) with no one leaving,
    /// so this stays `false`; in a game of three or more it is set exactly once,
    /// when the state-based-actions loop performs the leave-the-game cleanup for a
    /// player who lost while the game continues. Never unset. Engine-internal
    /// bookkeeping so the cleanup and its log event fire once, not per SBA pass.
    pub left_game: bool,
    /// Whether this player has attempted to draw from an empty library since the
    /// last time state-based actions were checked (CR 704.5c). Raw stored event,
    /// set by [`Self::draw`] and consumed by the state-based-actions loop, which
    /// turns it into a loss and clears it. Not a derivation — nothing else in the
    /// state determines it.
    pub attempted_draw_from_empty: bool,
    /// The player's deck (private, ordered).
    pub library: Vec<CardInstance>,
    /// Cards in the player's hand (private).
    pub hand: Vec<CardInstance>,
    /// The player's graveyard (public, ordered).
    pub graveyard: Vec<CardInstance>,
    /// Cards this player owns in exile.
    pub exile: Vec<CardInstance>,
    /// The player's command zone (CR 408), a **public** zone that holds their
    /// commander while it is there (CR 903.6). Empty for a player with no
    /// designated commander — the whole zone model is inert unless a
    /// [`GameSetup`](crate::GameSetup) designates one, so a non-commander game is
    /// byte-for-byte unchanged. Ordered like the other piles for a stable view.
    pub command: Vec<CardInstance>,
    /// This player's commander designation and the per-designation bookkeeping
    /// that outlives every object the commander becomes — the commander tax count
    /// (CR 903.8) and the pending return-to-command-zone decision (CR 903.9a).
    /// `None` for a player with no commander. Kept here, not on any battlefield
    /// object, because a recast commander is a fresh object but the same
    /// designation (see [`crate::commander`]).
    pub commander: Option<CommanderState>,
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

    /// Draw the top card of the library into hand (CR 120.1). If the library is
    /// empty, record the *attempted* draw (CR 704.5c) so the state-based-actions
    /// loop makes this player lose. Returns whether a card was actually drawn.
    ///
    /// This is the single choke point for every library draw (the turn-based draw
    /// step and card-draw effects both route through it), so decking is detected
    /// uniformly wherever a draw happens.
    pub fn draw(&mut self) -> bool {
        match self.library.pop() {
            Some(card) => {
                self.hand.push(card);
                true
            }
            None => {
                self.attempted_draw_from_empty = true;
                false
            }
        }
    }

    /// Borrow one of the player's private zones by name.
    #[must_use]
    pub fn zone(&self, zone: Zone) -> &Vec<CardInstance> {
        match zone {
            Zone::Library => &self.library,
            Zone::Hand => &self.hand,
            Zone::Graveyard => &self.graveyard,
            Zone::Exile => &self.exile,
            Zone::Command => &self.command,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::fixture;
    use crate::id::CardInstanceId;

    #[test]
    fn player_zone_accessor_matches_fields() {
        let hand_card = CardInstance {
            id: CardInstanceId(7),
            card: fixture("lightning_strike"),
        };
        let grave_card = CardInstance {
            id: CardInstanceId(9),
            card: fixture("shock"),
        };
        let mut player = Player::new();
        player.hand.push(hand_card);
        player.graveyard.push(grave_card);
        assert_eq!(player.zone(Zone::Hand), &vec![hand_card]);
        assert_eq!(player.zone(Zone::Graveyard), &vec![grave_card]);
        assert!(player.zone(Zone::Library).is_empty());
        assert!(player.zone(Zone::Exile).is_empty());
    }
}
