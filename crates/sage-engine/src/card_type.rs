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

/// Render a display type line from structured types, e.g. `"Basic Land — Forest"` or
/// `"Creature — Elf Scout"`. Supertypes and types are joined with spaces; subtypes, if
/// any, follow an em dash.
///
/// The single source for the string, shared by [`crate::CardData::type_line`] and
/// [`crate::PrintedFace::type_line`] so a token's type line is built exactly as a
/// card's is. It is never parsed back into types.
#[must_use]
pub(crate) fn render_type_line(
    supertypes: &[Supertype],
    types: &[CardType],
    subtypes: &[String],
) -> String {
    let mut head: Vec<&str> = Vec::new();
    head.extend(supertypes.iter().map(|s| s.display()));
    head.extend(types.iter().map(|t| t.display()));
    let mut line = head.join(" ");
    if !subtypes.is_empty() {
        line.push_str(" — ");
        line.push_str(&subtypes.join(" "));
    }
    line
}
