//! Zone identifiers.

/// One of a player's four private zones.
///
/// The shared battlefield is not listed here because it is owned by the game,
/// not by a player (see [`crate::GameState::battlefield`]).
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
}
