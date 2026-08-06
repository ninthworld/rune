//! A card's faces (CR 712): which one is up, and what the other one says.
//!
//! Most cards have one face and this module costs them nothing. A **transforming
//! double-faced card** has two, in printed order: the front face — which is
//! [`CardData`](super::CardData) itself, so every existing definition is already a
//! one-face card with nothing to migrate — and a [`BackFace`], authored under
//! `back_face`.
//!
//! Three facts shape the model, and each of them is a rule rather than a convenience:
//!
//! - **A [`FunctionalId`](crate::FunctionalId) names the card, not a face** (ADR 0008 §3).
//!   A printing, a decklist, and the compatibility report all name one identity for a
//!   two-faced card, exactly as a real set does — so nothing about `data/sets/` changes.
//! - **The back face has no mana cost and is never cast** (CR 712.4a). It carries no
//!   `mana_cost` worth the name, no `spell_effects`, no `additional_cost`, and no
//!   `attachment`: three of those four are absent *fields*, so writing one is a parse
//!   error, and the fourth is checked by the catalog validator
//!   ([`Violation::BackFaceHasManaCost`](crate::Violation)) because the field has to
//!   exist for the rule to be enforced rather than assumed.
//! - **Which face is up is a fact about the permanent**, carried on
//!   [`Printed::Card`](crate::Printed) and turned over by
//!   [`Effect::TransformSelf`](crate::Effect). A card anywhere but the battlefield has
//!   only its front face's characteristics (CR 712.4a), which is why nothing outside
//!   the battlefield stores a [`Face`] at all.

use serde::Deserialize;

use super::keyword::Keyword;
use super::restriction::CombatRestriction;
use crate::ability::Ability;
use crate::card_type::{CardType, Supertype};
use crate::mana::Color;

/// Which face of a card is up (CR 712.4) — a position in the card's ordered face list
/// ([`CardData::faces`](super::CardData::faces)).
///
/// Two variants rather than an index, because two is what a transforming double-faced
/// card has and an out-of-range index is a state the compiler should not have to be
/// asked about. A third face would be a variant here and an arm in every match that
/// consumes one, which is the same forcing function every other closed set in the
/// engine has.
///
/// [`Front`](Self::Front) is the [`Default`], and that is load-bearing in one place: a
/// permanent built from a card with no back face is front-face up without anything
/// having to say so.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Face {
    /// The front face — the one a card is cast as, and the only one a card outside the
    /// battlefield has (CR 712.4a).
    #[default]
    Front,
    /// The back face — the one a permanent shows after it transforms (CR 712.2a).
    Back,
}

impl Face {
    /// The other face (CR 701.28a): what transforming turns this one into.
    ///
    /// Total, and deliberately so: a card with no back face never reaches a transform,
    /// because the effect that would turn it over checks
    /// [`CardData::has_back_face`](super::CardData::has_back_face) first.
    #[must_use]
    pub fn other(self) -> Self {
        match self {
            Self::Front => Self::Back,
            Self::Back => Self::Front,
        }
    }
}

/// The **back face** of a transforming double-faced card (CR 712.2) — everything the
/// permanent is once it has been turned over.
///
/// The counterpart of [`CardData`](super::CardData) for the face that is not the card's
/// identity. It carries the characteristics a permanent has, and nothing about *casting*:
/// a back face is never cast (CR 712.4a), so there is no `spell_effects`, no
/// `additional_cost`, and no `attachment` field for an author to fill in. That absence is
/// the enforcement — `deny_unknown_fields` makes each of them a parse error — and the one
/// rule that could not be expressed as an absent field, the empty mana cost, is checked by
/// the catalog validator instead ([`Violation::BackFaceHasManaCost`](crate::Violation)).
///
/// It carries no `functional_id` for the same reason [`TokenData`](crate::TokenData)
/// does not: identity belongs to the card, and a face that claimed its own would put a
/// second entry in the catalog's identity space for one physical card.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackFace {
    /// The face's name (e.g. `"Nicol Bolas, the Arisen"`). A double-faced card's two
    /// faces have different names, and the one that is up is the permanent's name.
    pub name: String,
    /// Printed supertypes of this face (e.g. `Legendary`); empty for most.
    #[serde(default)]
    pub supertypes: Vec<Supertype>,
    /// Printed card types of this face. Every face has at least one, and on a back face
    /// they are frequently not the front's — a creature that transforms into a
    /// planeswalker changes type by turning over.
    pub types: Vec<CardType>,
    /// Printed subtypes of this face; empty for many.
    #[serde(default)]
    pub subtypes: Vec<String>,
    /// The face's mana cost, which must be **empty** (CR 712.4a): a back face has no
    /// mana cost and can never be cast.
    ///
    /// The field exists so the rule can be *enforced* rather than assumed — an author
    /// who writes one gets [`Violation::BackFaceHasManaCost`](crate::Violation) naming
    /// the card, which is a better answer than a field that silently does not exist.
    ///
    /// A back face's mana **value** is a different question, and it is not zero:
    /// CR 712.4d gives it the front face's, which is why
    /// [`PrintedFace::CardBack`](crate::PrintedFace) carries the card beside the face.
    #[serde(default)]
    pub mana_cost: String,
    /// The face's colors (CR 105.2); empty for a colorless face. Authored explicitly,
    /// exactly as [`CardData::colors`](super::CardData::colors) is — and here it has to
    /// be, since the face has no cost whose pips could imply them.
    #[serde(default)]
    pub colors: Vec<Color>,
    /// Printed power, when this face is a creature; `None` otherwise.
    #[serde(default)]
    pub power: Option<i32>,
    /// Printed toughness, when this face is a creature; `None` otherwise.
    #[serde(default)]
    pub toughness: Option<i32>,
    /// Printed **starting loyalty**, when this face is a planeswalker (CR 306.5b);
    /// `None` otherwise.
    ///
    /// A permanent that arrives with this face up enters with this many loyalty counters,
    /// at the same battlefield-entry seam a front-face planeswalker's are applied. A
    /// permanent that *transforms* into this face on the battlefield does **not** — it is
    /// the same object (CR 712.a), and CR 306.5b speaks about entering.
    #[serde(default)]
    pub loyalty: Option<u32>,
    /// The face's abilities as declarative data — read only while this face is up
    /// (CR 712.4b), so a transforming permanent's activations, triggers, and static
    /// abilities are the ones printed on the side facing the table.
    #[serde(default)]
    pub abilities: Vec<Ability>,
    /// The face's printed keyword abilities (CR 702); empty for a face with none.
    #[serde(default)]
    pub keywords: Vec<Keyword>,
    /// The face's printed combat restrictions (CR 506.3, CR 509.1b); empty for nearly
    /// every face.
    #[serde(default)]
    pub restrictions: Vec<CombatRestriction>,
}

impl BackFace {
    /// Render this face's printed type line for display, e.g.
    /// `"Legendary Planeswalker — Bolas"`. The same single routine
    /// [`CardData::type_line`](super::CardData::type_line) uses, so the two faces of one
    /// card are rendered identically.
    #[must_use]
    pub fn type_line(&self) -> String {
        crate::card_type::render_type_line(&self.supertypes, &self.types, &self.subtypes)
    }

    /// Whether this face has printed card type `card_type`.
    #[must_use]
    pub fn has_type(&self, card_type: CardType) -> bool {
        self.types.contains(&card_type)
    }
}
