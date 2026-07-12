//! Lightweight identity newtypes.
//!
//! Four layers of identity, only two of them authored by a human (ADR 0018 §3):
//!
//! | Layer | Type | Assigned by | Stable for |
//! |---|---|---|---|
//! | Functional (the card as a rules object) | [`FunctionalId`] | the card's author, by hand | forever — never reused or renumbered |
//! | Interned handle | [`CardId`], aliased [`OracleId`] | the catalog loader | one build |
//! | Printing | set code + collector number | the set file listing it, by hand | forever |
//! | Per-game instance | [`CardInstanceId`], [`PermanentId`] | [`crate::GameState::mint_id`] | one game (a `PermanentId`: one battlefield stay) |
//!
//! A printing record and a decklist name a [`FunctionalId`]; the loader resolves
//! it to that build's [`CardId`], which is what every rules read then goes
//! through. The printing identity never enters [`crate::GameState`] — reprints are
//! rules-identical, so the engine cannot tell which one a copy was opened from
//! (ADR 0013 §1).

use std::fmt;

use serde::Deserialize;

/// The authored, stable identity of one functional card definition (ADR 0018 §3).
///
/// A lowercase `snake_case` slug (e.g. `thornback_boar`), assigned once by
/// whoever writes the card and never reused or renumbered. This — not the
/// interned [`CardId`] — is what a printing record, a decklist, and any future
/// external mapping reference, because it is the only card identity that is
/// stable across builds.
///
/// Constructed only through [`TryFrom<String>`], so an ill-formed slug (uppercase,
/// spaces, a leading digit, a doubled or trailing underscore) cannot exist: a card
/// file carrying one fails to load rather than interning a malformed identity.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Deserialize)]
#[serde(try_from = "String")]
pub struct FunctionalId(String);

impl FunctionalId {
    /// The slug as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for FunctionalId {
    type Error = FunctionalIdError;

    fn try_from(slug: String) -> Result<Self, Self::Error> {
        let well_formed = !slug.is_empty()
            && slug.starts_with(|c: char| c.is_ascii_lowercase())
            && !slug.ends_with('_')
            && !slug.contains("__")
            && slug
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
        if well_formed {
            Ok(Self(slug))
        } else {
            Err(FunctionalIdError(slug))
        }
    }
}

impl fmt::Display for FunctionalId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A string that is not a well-formed [`FunctionalId`] slug.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FunctionalIdError(String);

impl fmt::Display for FunctionalIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:?} is not a functional id: expected lowercase snake_case, \
             starting with a letter (e.g. \"thornback_boar\")",
            self.0
        )
    }
}

impl std::error::Error for FunctionalIdError {}

/// A build-local handle to one functional definition — the key every rules read
/// goes through.
///
/// Interned by the catalog loader, never hand-written into a data file: it is a
/// *handle to* the authored [`FunctionalId`], not the authored identity itself.
/// It is therefore stable for the life of a build (matching ADR 0002's in-memory,
/// never-persisted `GameState`) and nothing outside that build may assume a given
/// card keeps the same integer.
///
/// A card keeps the same `CardId` in every zone; it is not the battlefield
/// identity (see [`PermanentId`]) nor the per-copy identity (see
/// [`CardInstanceId`]). Two copies of one card share one `CardId`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct CardId(pub u64);

/// Documentary alias for a [`CardId`] read as "the functional definition a
/// printing resolves to" (ADR 0013 §1).
///
/// Printings carry no rules; each names a [`FunctionalId`] that the loader
/// resolves to this build's interned handle, and every rules read then goes
/// through that handle. Same type, not a distinct one — the distinct, *authored*
/// identity is [`FunctionalId`].
pub type OracleId = CardId;

/// Identifies one physical card in a game, distinct from every other copy.
///
/// Minted fresh from [`crate::GameState::mint_id`] when a card first enters a
/// zone, so two copies of the same [`CardId`] (two Forests in hand) are
/// individually addressable in library, hand, graveyard, exile, and on the
/// stack. Unlike [`PermanentId`], which is reborn on each battlefield entry, an
/// instance id stays with the physical card as it moves between those zones.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct CardInstanceId(pub u64);

/// One physical card occupying a zone: its per-game [`CardInstanceId`] paired
/// with the [`CardId`] of the card it is a copy of.
///
/// This pairing is the mapping from instance identity to card definition that lets
/// duplicate copies stay distinguishable within a zone (`Vec<CardInstance>`),
/// rather than collapsing to a bare `CardId`. It holds no printing identity: two
/// copies opened from different printings of one card are indistinguishable here,
/// deliberately, because no rule may depend on which printing a copy came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct CardInstance {
    /// This copy's unique per-game identity.
    pub id: CardInstanceId,
    /// The card this copy represents.
    pub card: CardId,
}

/// Identifies a seat at the table by index into [`crate::GameState::players`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct PlayerId(pub usize);

/// Fresh identity assigned to a permanent each time it enters the battlefield.
///
/// Zone-change identity is the mechanism: the "second time on the battlefield"
/// has a different `PermanentId` than the first, so there are no zone-change
/// counters (see `crates/rune-engine/AGENTS.md`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct PermanentId(pub u64);

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn a_well_formed_slug_is_accepted() {
        let id = FunctionalId::try_from("thornback_boar".to_string()).unwrap();
        assert_eq!(id.as_str(), "thornback_boar");
        assert_eq!(id.to_string(), "thornback_boar");
        // Digits are allowed after the first character.
        assert!(FunctionalId::try_from("scout_2".to_string()).is_ok());
    }

    #[test]
    fn an_ill_formed_slug_is_rejected() {
        // ADR 0018 §3: lowercase snake_case, starting with a letter. Anything else
        // is not an identity a card file may claim.
        for slug in [
            "",
            "Thornback_Boar",  // uppercase
            "thornback boar",  // space
            "thornback-boar",  // kebab
            "2thornback",      // leading digit
            "_thornback",      // leading underscore
            "thornback_",      // trailing underscore
            "thornback__boar", // doubled underscore
            "thornbäck_boar",  // non-ASCII
        ] {
            assert!(
                FunctionalId::try_from(slug.to_string()).is_err(),
                "{slug:?} should not be a functional id"
            );
        }
    }

    #[test]
    fn the_rejection_message_names_the_offending_slug() {
        let err = FunctionalId::try_from("Thornback Boar".to_string()).unwrap_err();
        let message = err.to_string();
        assert!(message.contains("Thornback Boar"), "{message}");
        assert!(message.contains("snake_case"), "{message}");
    }
}
