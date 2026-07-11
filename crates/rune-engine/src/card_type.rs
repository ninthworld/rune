//! Card types and supertypes: the structured type line the engine reasons about.
//!
//! Rules key off these values, never off a parsed display string. The two closed
//! sets — [`CardType`] and [`Supertype`] — are enums; subtypes are an open
//! `Vec<String>` on [`crate::CardData`] because there are thousands of them.
//! These are the card's *printed* types; type-changing continuous effects (the
//! layer system, later) derive a permanent's current types from these.

use serde::Deserialize;

/// A card's primary type (CR 300). Closed set, deserialized from lowercase names
/// (e.g. `"creature"`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CardType {
    /// Land.
    Land,
    /// Creature.
    Creature,
    /// Artifact.
    Artifact,
    /// Enchantment.
    Enchantment,
    /// Instant.
    Instant,
    /// Sorcery.
    Sorcery,
    /// Planeswalker.
    Planeswalker,
    /// Battle.
    Battle,
}

impl CardType {
    /// The word as it appears in a rendered type line (e.g. `"Creature"`).
    #[must_use]
    pub fn display(self) -> &'static str {
        match self {
            Self::Land => "Land",
            Self::Creature => "Creature",
            Self::Artifact => "Artifact",
            Self::Enchantment => "Enchantment",
            Self::Instant => "Instant",
            Self::Sorcery => "Sorcery",
            Self::Planeswalker => "Planeswalker",
            Self::Battle => "Battle",
        }
    }
}

/// A card's supertype (CR 205.4). Closed set, deserialized from lowercase names
/// (e.g. `"basic"`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Supertype {
    /// Basic (as on basic lands).
    Basic,
    /// Legendary.
    Legendary,
    /// Snow.
    Snow,
    /// World.
    World,
}

impl Supertype {
    /// The word as it appears in a rendered type line (e.g. `"Basic"`).
    #[must_use]
    pub fn display(self) -> &'static str {
        match self {
            Self::Basic => "Basic",
            Self::Legendary => "Legendary",
            Self::Snow => "Snow",
            Self::World => "World",
        }
    }
}
