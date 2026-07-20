//! Zone identifiers.

/// One of a player's owned zones.
///
/// Most are private to the player; [`Zone::Command`] is a **public** zone
/// (CR 408) holding their commander. The shared battlefield is not listed here
/// because it is owned by the game, not by a player (see
/// [`crate::GameState::battlefield`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Zone {
    /// The player's deck, face down.
    Library,
    /// Cards in the player's hand.
    Hand,
    /// The player's discard pile, face up.
    Graveyard,
    /// Cards the player owns that have been exiled.
    Exile,
    /// The player's command zone (CR 408): a public zone that holds their
    /// commander while it is there (CR 903.6). Empty for a player with no
    /// designated commander.
    Command,
}
