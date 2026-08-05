//! Why a submitted decklist is illegal for a format, and how each reason reads.

use super::*;

/// Why a submitted decklist is illegal for a format. Distinct from
/// an *unknown card*, which the lobby rejects before legality is even considered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DeckError {
    /// The deck holds fewer than the format's minimum number of cards.
    BelowMinimum {
        /// How many cards the deck holds.
        have: usize,
        /// The format's minimum deck size.
        min: usize,
    },
    /// The deck holds more than the format's maximum number of cards.
    AboveMaximum {
        /// How many cards the deck holds.
        have: usize,
        /// The format's maximum deck size.
        max: usize,
    },
    /// A single non-exempt card appears more times than the format's copy limit.
    CopyLimit {
        /// The offending card.
        card: CardId,
        /// How many copies the deck holds.
        count: usize,
        /// The format's per-card copy limit.
        limit: usize,
    },
    /// A commander format requires a designated commander (CR 903.3) and the deck
    /// designated none.
    MissingCommander,
    /// The designated commander is not among the submitted deck's cards (CR 903.3).
    CommanderNotInDeck {
        /// The designated card the decklist does not contain.
        card: CardId,
    },
    /// The designated commander is not a legendary creature (CR 903.5a).
    CommanderNotLegendaryCreature {
        /// The designated card that is not a legendary creature.
        card: CardId,
    },
    /// A card's color identity is not contained in the commander's (CR 903.4).
    OutOfIdentity {
        /// The first card (in deck order) outside the commander's color identity.
        card: CardId,
    },
}

impl DeckError {
    /// The offending [`CardId`] this rejection is about, if any. Size and
    /// missing-commander rejections name no specific card and return `None`; every
    /// card-specific variant returns the card at fault.
    pub(crate) fn card(&self) -> Option<CardId> {
        match self {
            Self::BelowMinimum { .. } | Self::AboveMaximum { .. } | Self::MissingCommander => None,
            Self::CopyLimit { card, .. }
            | Self::CommanderNotInDeck { card }
            | Self::CommanderNotLegendaryCreature { card }
            | Self::OutOfIdentity { card } => Some(*card),
        }
    }

    /// A stable `snake_case` machine code for this rejection class, mirrored on the
    /// wire in [`LobbyRejection::code`] so a client can branch without parsing the
    /// human-readable reason (issue #395).
    pub(crate) fn code(&self) -> &'static str {
        match self {
            Self::BelowMinimum { .. } => "below_minimum",
            Self::AboveMaximum { .. } => "above_maximum",
            Self::CopyLimit { .. } => "copy_limit",
            Self::MissingCommander => "missing_commander",
            Self::CommanderNotInDeck { .. } => "commander_not_in_deck",
            Self::CommanderNotLegendaryCreature { .. } => "commander_not_legendary_creature",
            Self::OutOfIdentity { .. } => "out_of_identity",
        }
    }

    /// Render this rejection into its human-readable sentence, naming any offending
    /// card through `card_label`. This is the single source of the wording: the
    /// [`Display`](std::fmt::Display) impl labels a card by its raw [`CardId`] (for
    /// logs), while [`to_rejection`](DeckError::to_rejection) labels it by the card's
    /// display name (for the player). No new sentence templates are added anywhere —
    /// both callers reuse these (issue #395).
    fn render(&self, card_label: impl Fn(CardId) -> String) -> String {
        match self {
            Self::BelowMinimum { have, min } => {
                format!("deck has {have} cards, below the {min}-card minimum")
            }
            Self::AboveMaximum { have, max } => {
                format!("deck has {have} cards, above the {max}-card maximum")
            }
            Self::CopyLimit { card, count, limit } => format!(
                "{} appears {count} times, above the {limit}-copy limit",
                card_label(*card)
            ),
            Self::MissingCommander => "this format requires a designated commander".to_string(),
            Self::CommanderNotInDeck { card } => format!(
                "the designated commander ({}) is not in the deck",
                card_label(*card)
            ),
            Self::CommanderNotLegendaryCreature { card } => format!(
                "the designated commander ({}) is not a legendary creature",
                card_label(*card)
            ),
            Self::OutOfIdentity { card } => format!(
                "{} is outside the commander's color identity",
                card_label(*card)
            ),
        }
    }

    /// Project this rejection into the wire [`LobbyRejection`] delivered to the
    /// rejecting seat only (issue #395), resolving the offending [`CardId`] through
    /// `db` to name it by its display name in the reason and to carry its stable
    /// [`CardIdentity`] (`functional_id`). The reason reuses the same wording the
    /// server logs (see [`render`](DeckError::render)); nothing new is invented. The
    /// named card is always one of the sender's own submitted cards, so no other
    /// seat's hidden state leaks.
    pub(crate) fn to_rejection(&self, db: &CardDatabase) -> sage_protocol::LobbyRejection {
        let reason = self.render(|card| card_display_name(db, card));
        sage_protocol::LobbyRejection {
            code: self.code().to_string(),
            reason,
            card: self
                .card()
                .and_then(|card| db.card(card))
                .map(|data| data.functional_id.as_str().to_string()),
        }
    }
}

/// A card's display name for a player-facing message, read through `db`. Falls back
/// to the raw [`CardId`] label only if the id does not resolve (the lobby rejects
/// unknown ids before deck legality, so this is a defensive default).
pub(super) fn card_display_name(db: &CardDatabase, card: CardId) -> String {
    db.card(card)
        .map(|data| data.name.clone())
        .unwrap_or_else(|| format!("card {}", card.0))
}

impl std::fmt::Display for DeckError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.render(|card| format!("card {}", card.0)))
    }
}

impl std::error::Error for DeckError {}

/// Whether `card` is a basic land — carries the engine's structured
/// [`Supertype::Basic`] — read through `db`. The basic-land *policy* (exemption
/// from the copy limit) lives in [`Format::validate_deck`]; only this datum is the
/// engine's. An unknown id is treated as non-basic; the lobby has
/// already rejected unknown ids before legality is checked.
pub(super) fn is_basic(db: &CardDatabase, card: CardId) -> bool {
    db.card(card)
        .is_some_and(|data| data.supertypes.contains(&Supertype::Basic))
}
