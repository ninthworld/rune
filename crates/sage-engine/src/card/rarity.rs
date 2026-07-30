//! Printing rarity.

use serde::Deserialize;

/// A card's rarity in a given printing (ADR 0013 §1).
///
/// A purely bibliographic property of the printing, not a rule the engine reasons
/// about. Serialized lowercase to mirror Scryfall's data shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Rarity {
    /// Common.
    Common,
    /// Uncommon.
    Uncommon,
    /// Rare.
    Rare,
    /// Mythic rare.
    Mythic,
}
