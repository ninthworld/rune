//! Tokens: a permanent that is not a card (CR 111).
//!
//! Everything else on the battlefield is a card that moved there, so its
//! characteristics are read out of the catalog through its
//! [`CardId`](crate::CardId). A token has no card behind it — the effect that created
//! it *is* its printed face (CR 111.3) — and it exists only on the battlefield: the
//! instant it would go anywhere else it ceases to exist (CR 111.7).
//!
//! Three types carry that:
//!
//! - [`TokenData`] — the characteristics an effect gives a token it creates. Authored
//!   inline on [`Effect::CreateToken`](crate::Effect::CreateToken), so it deliberately
//!   has **no `functional_id`**: a token is not a catalog card, is not decklist-legal,
//!   and must never appear in the compatibility report.
//! - [`Printed`] — the stored field on a [`Permanent`](crate::Permanent): where that
//!   permanent's printed characteristics come from, a catalog card or a token. It
//!   replaces the bare `CardId` the type used to carry, so every read of a permanent's
//!   printed face is a place the compiler makes answer the token question.
//! - [`PrintedFace`] — the borrowed union the read paths use. One `match` inside this
//!   module answers "types", "power", "keywords" … for both kinds, so the dozen call
//!   sites that used to write `db.card(perm.card)` read one shape instead of two.
//!
//! Abilities are the one characteristic [`PrintedFace`] does not expose: a card's
//! ability set unions the data tier with the code tier keyed on its authored identity
//! (`crate::scripted`), which a token has none of. Read a permanent's abilities through
//! [`abilities_of_permanent`](crate::card::abilities_of_permanent), which answers both
//! kinds.

use serde::Deserialize;

use crate::ability::Ability;
use crate::card::{Attachment, BackFace, CardData, CombatRestriction, Face, Keyword};
use crate::card_type::{render_type_line, CardType, Supertype};
use crate::id::CardId;
use crate::mana::Color;

/// The characteristics a token is created with (CR 111.3) — the token's whole printed
/// face, defined by the effect that creates it rather than by a card.
///
/// Authored inline in card data on [`Effect::CreateToken`](crate::Effect::CreateToken),
/// e.g. `{"name":"Goblin","types":["creature"],"subtypes":["Goblin"],
/// "colors":["red"],"power":1,"toughness":1}`.
///
/// What is **absent** here is as deliberate as what is present. There is no
/// `functional_id`: a token is not a card (CR 111), so it has no authored identity,
/// cannot be named by a decklist or a printing, and cannot appear in the
/// compatibility report — none of which is a rule to be remembered, because the field
/// does not exist to fill in. There is no mana cost (a token has none unless an effect
/// says otherwise), no `spell_effects` (a token is never cast), no `attachment` grant, and
/// no `scripted` escape hatch (code-defined behavior is keyed on an authored identity
/// this type has not got). `deny_unknown_fields` makes each of those a parse error
/// rather than a silently ignored field, exactly as it does on
/// [`CardData`](crate::CardData).
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TokenData {
    /// The token's name (e.g. `"Goblin"`, `"Thopter"`). Tokens are named for what
    /// they are, so the name usually repeats the creature subtype.
    pub name: String,
    /// The token's card types. Every token has at least one, and each must be a
    /// permanent type — a token that is not a permanent could never exist on the
    /// battlefield, which is the only zone a token may be in (CR 111.7). Enforced by
    /// the catalog validator.
    pub types: Vec<CardType>,
    /// The token's subtypes (e.g. `"Goblin"`, `"Soldier"`); empty for a token with
    /// none.
    #[serde(default)]
    pub subtypes: Vec<String>,
    /// The token's colors (CR 105.2); empty for a colorless token, which is what an
    /// artifact token usually is.
    #[serde(default)]
    pub colors: Vec<Color>,
    /// The token's power, for a creature token; `None` for a noncreature token.
    #[serde(default)]
    pub power: Option<i32>,
    /// The token's toughness, for a creature token; `None` for a noncreature token.
    #[serde(default)]
    pub toughness: Option<i32>,
    /// The token's keyword abilities (CR 702) — flying on a Thopter, vigilance on a
    /// Knight. Empty for a token with none.
    #[serde(default)]
    pub keywords: Vec<Keyword>,
    /// The token's combat restrictions (CR 506.3, CR 509.1b); empty for nearly every
    /// token, exactly as it is for nearly every card.
    #[serde(default)]
    pub restrictions: Vec<CombatRestriction>,
    /// The token's non-keyword abilities — a token that carries an activated or
    /// triggered ability of its own (a Dragon token with firebreathing). Empty for the
    /// ordinary vanilla token.
    ///
    /// Data-driven only: the code tier ([`crate::scripted`]) is keyed on an authored
    /// `functional_id`, which a token has not got, so a token's abilities are exactly
    /// what the creating effect wrote down.
    #[serde(default)]
    pub abilities: Vec<Ability>,
}

impl TokenData {
    /// Whether this token has card type `card_type`.
    #[must_use]
    pub fn has_type(&self, card_type: CardType) -> bool {
        self.types.contains(&card_type)
    }
}

/// Where a [`Permanent`](crate::Permanent)'s printed characteristics come from: the
/// catalog card it represents, or — for a token (CR 111) — the characteristics the
/// effect that created it gave it.
///
/// This is the field a permanent stores in place of the bare
/// [`CardId`](crate::CardId) it used to. That is the point: a permanent's printed face
/// is no longer *always* a card, and every read of it now goes through a value that
/// says so. Code that needs the card behind a permanent — to move it to a graveyard,
/// to name it in the log — asks [`Self::card`] and must handle the `None` that a token
/// answers with, which is precisely where CR 111.7 lives.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Printed {
    /// A catalog card (CR 110.1): the ordinary permanent, whose characteristics are
    /// read from the [`CardDatabase`](crate::CardDatabase).
    Card {
        /// The catalog card this permanent represents.
        card: CardId,
        /// **Which face is up** (CR 712.4). [`Face::Front`] for every single-faced card
        /// and for a two-faced one that has not turned over; transforming
        /// ([`Effect::TransformSelf`](crate::Effect)) changes this field and nothing
        /// else, which is the whole of CR 712.a — the permanent's counters, damage,
        /// attachments, and combat state are untouched because they live beside this,
        /// on the same object.
        face: Face,
    },
    /// A token (CR 111): no card, characteristics carried on the object itself. Boxed
    /// so the common variant does not pay for the rare one — a [`Permanent`](crate::Permanent)
    /// is cloned on every [`apply_action`](crate::apply_action).
    Token(Box<TokenData>),
}

impl Default for Printed {
    /// The default [`Permanent`](crate::Permanent) represents the default card, which
    /// is what it did before tokens existed. A token is never a default — it is always
    /// created by an effect that says what it is.
    fn default() -> Self {
        Self::Card {
            card: CardId::default(),
            face: Face::Front,
        }
    }
}

impl From<CardId> for Printed {
    /// A card, **front face up** (CR 712.4a) — the state every permanent of a card
    /// starts in, whether or not the card has a second face.
    fn from(card: CardId) -> Self {
        Self::Card {
            card,
            face: Face::Front,
        }
    }
}

impl Printed {
    /// The catalog card behind this permanent, or `None` for a token.
    ///
    /// The one accessor that crosses back to card identity, and the seam CR 111.7 is
    /// enforced at: a caller moving a permanent to another zone gets a
    /// [`CardInstance`](crate::CardInstance) to put there only when there is a card,
    /// and a token — having none — is simply not put anywhere and so ceases to exist.
    #[must_use]
    pub fn card(&self) -> Option<CardId> {
        match self {
            Self::Card { card, .. } => Some(*card),
            Self::Token(_) => None,
        }
    }

    /// Which face of this permanent is up (CR 712.4). [`Face::Front`] for a token,
    /// which has exactly one face — the effect that created it (CR 111.3).
    #[must_use]
    pub fn face_up(&self) -> Face {
        match self {
            Self::Card { face, .. } => *face,
            Self::Token(_) => Face::Front,
        }
    }

    /// Turn this permanent over (CR 701.28a), leaving everything else about it alone.
    ///
    /// Returns `false` — and changes nothing — when there is no other face to turn to:
    /// a single-faced card and a token both stay exactly as they were, which is what
    /// CR 701.28d says about a permanent that is not a transforming double-faced
    /// permanent.
    pub(crate) fn transform(&mut self, db: &crate::CardDatabase) -> bool {
        let Self::Card { card, face } = self else {
            return false;
        };
        if !db.card(*card).is_some_and(CardData::has_back_face) {
            return false;
        }
        *face = face.other();
        true
    }

    /// Whether this permanent is a token (CR 111) — the first-class predicate the
    /// "nontoken" wording of many cards needs, rather than an absence to be inferred.
    #[must_use]
    pub fn is_token(&self) -> bool {
        matches!(self, Self::Token(_))
    }

    /// This permanent's printed face: the catalog entry for a card, the carried
    /// characteristics for a token, or `None` when a card handle is not in `db` (the
    /// unknown-id case the engine surfaces rather than panics on).
    #[must_use]
    pub fn face<'a>(&'a self, db: &'a crate::CardDatabase) -> Option<PrintedFace<'a>> {
        match self {
            Self::Card { card, face } => db.card(*card).and_then(|data| data.face(*face)),
            Self::Token(token) => Some(PrintedFace::Token(token)),
        }
    }
}

/// A permanent's printed characteristics, borrowed from whichever source has them.
///
/// The read-side counterpart of [`Printed`], produced by [`Printed::face`]. Its
/// accessors are the union of what the rules paths actually read off a printed face,
/// each answering both kinds in one place — so a combat check, a targeting check, and
/// the view projection all treat a token exactly as they treat a card, without any of
/// them knowing which they hold.
///
/// Borrowed rather than owned: a permanent's face is read on every characteristics
/// computation, every combat legality check, and every view projection, and cloning a
/// name and three vectors for each of those would be a tax paid for nothing.
#[derive(Clone, Copy, Debug)]
pub enum PrintedFace<'a> {
    /// The **front** face of a permanent that is a card — which is the card itself
    /// (CR 712.2), and the only face a single-faced card has.
    Card(&'a CardData),
    /// The **back** face of a transforming double-faced permanent that has turned over
    /// (CR 712.4b).
    ///
    /// A third variant rather than a flag on [`Self::Card`], for the reason
    /// [`Self::Token`] is one: the two faces are two different objects' worth of
    /// characteristics, and every accessor below answers all three in one place — so a
    /// combat check, a targeting check, and the view projection read a transformed
    /// permanent exactly as they read an untransformed one, without any of them knowing
    /// which face they hold.
    ///
    /// It carries the **card** as well as the face, because two questions about a back
    /// face are answered by the front: its mana value (CR 712.4d — a back face has no
    /// mana cost but takes the front's mana value) and the card's authored identity,
    /// which belongs to the card rather than to either face.
    CardBack {
        /// The card this face belongs to — its identity, and the front face whose mana
        /// value CR 712.4d borrows.
        card: &'a CardData,
        /// The face's own printed characteristics.
        face: &'a BackFace,
    },
    /// The face of a token, from the effect that created it.
    Token(&'a TokenData),
}

impl<'a> PrintedFace<'a> {
    /// The object's name.
    #[must_use]
    pub fn name(&self) -> &'a str {
        match self {
            Self::Card(card) => &card.name,
            Self::CardBack { face, .. } => &face.name,
            Self::Token(token) => &token.name,
        }
    }

    /// The printed supertypes. Always empty for a token: no token the effect IR can
    /// create is basic or legendary, so [`TokenData`] has no field for one.
    #[must_use]
    pub fn supertypes(&self) -> &'a [Supertype] {
        match self {
            Self::Card(card) => &card.supertypes,
            Self::CardBack { face, .. } => &face.supertypes,
            Self::Token(_) => &[],
        }
    }

    /// The printed card types.
    #[must_use]
    pub fn types(&self) -> &'a [CardType] {
        match self {
            Self::Card(card) => &card.types,
            Self::CardBack { face, .. } => &face.types,
            Self::Token(token) => &token.types,
        }
    }

    /// The printed subtypes.
    #[must_use]
    pub fn subtypes(&self) -> &'a [String] {
        match self {
            Self::Card(card) => &card.subtypes,
            Self::CardBack { face, .. } => &face.subtypes,
            Self::Token(token) => &token.subtypes,
        }
    }

    /// The printed mana cost in curly-brace notation. Always empty for a token, which
    /// has no mana cost (CR 111.3 — a token has only the characteristics its creating
    /// effect defines), and always empty for a **back face** (CR 712.4a), which the
    /// catalog validator enforces. A back face's mana *value* is a separate question —
    /// see [`Self::mana_value`].
    #[must_use]
    pub fn mana_cost(&self) -> &'a str {
        match self {
            Self::Card(card) => &card.mana_cost,
            // CR 712.4a: a back face has no mana cost, which the catalog validator
            // enforces — so this reads an authored field that is always empty.
            Self::CardBack { face, .. } => &face.mana_cost,
            Self::Token(_) => "",
        }
    }

    /// The object's **mana value** (CR 202.3): the total of its printed mana cost.
    ///
    /// Zero for a token, and for anything else with no mana cost, which CR 202.3b says
    /// outright — so a class measured by mana value never has to decide whether to skip
    /// a token, and simply finds a zero where one is. The one exception is a **back
    /// face**, which has no mana cost and yet takes its front face's mana value
    /// (CR 712.4d).
    #[must_use]
    pub fn mana_value(&self) -> u32 {
        // CR 712.4d: the back face of a transforming double-faced permanent has no mana
        // cost of its own, and its mana value is its **front face's** — so a transformed
        // Elder Dragon is still mana value 4, not 0. Every other face reads its own cost.
        let cost = match self {
            Self::CardBack { card, .. } => crate::mana::parse_mana_cost(&card.mana_cost),
            _ => crate::mana::parse_mana_cost(self.mana_cost()),
        };
        u32::from(cost.generic) + u32::from(cost.colored_total())
    }

    /// The object's colors (CR 105.2).
    #[must_use]
    pub fn colors(&self) -> &'a [Color] {
        match self {
            Self::Card(card) => &card.colors,
            Self::CardBack { face, .. } => &face.colors,
            Self::Token(token) => &token.colors,
        }
    }

    /// The printed power, for a creature; `None` otherwise.
    #[must_use]
    pub fn power(&self) -> Option<i32> {
        match self {
            Self::Card(card) => card.power,
            Self::CardBack { face, .. } => face.power,
            Self::Token(token) => token.power,
        }
    }

    /// The printed toughness, for a creature; `None` otherwise.
    #[must_use]
    pub fn toughness(&self) -> Option<i32> {
        match self {
            Self::Card(card) => card.toughness,
            Self::CardBack { face, .. } => face.toughness,
            Self::Token(token) => token.toughness,
        }
    }

    /// The printed keyword abilities (CR 702) — the *seed* for the computed set, which
    /// folds in continuous grants at CR 613 layer 6.
    #[must_use]
    pub fn keywords(&self) -> &'a [Keyword] {
        match self {
            Self::Card(card) => &card.keywords,
            Self::CardBack { face, .. } => &face.keywords,
            Self::Token(token) => &token.keywords,
        }
    }

    /// The printed **starting loyalty**, for a planeswalker; `None` otherwise
    /// (CR 306.5b). Always `None` for a token: the effect IR creates no planeswalker
    /// token, so [`TokenData`] has no field for one — the same reason
    /// [`Self::supertypes`] is always empty.
    #[must_use]
    pub fn loyalty(&self) -> Option<u32> {
        match self {
            Self::Card(card) => card.loyalty,
            Self::CardBack { face, .. } => face.loyalty,
            Self::Token(_) => None,
        }
    }

    /// The printed combat restrictions (CR 506.3, CR 509.1b) — likewise the seed for
    /// the computed set.
    #[must_use]
    pub fn restrictions(&self) -> &'a [CombatRestriction] {
        match self {
            Self::Card(card) => &card.restrictions,
            Self::CardBack { face, .. } => &face.restrictions,
            Self::Token(token) => &token.restrictions,
        }
    }

    /// The attachment ability of an Aura (CR 303.4) or an Equipment (CR 301.5), or
    /// `None`. Always `None` for a token: no effect in the IR creates one that attaches
    /// to anything, so [`TokenData`] carries no grant.
    #[must_use]
    pub fn attachment(&self) -> Option<&'a Attachment> {
        match self {
            Self::Card(card) => card.attachment.as_ref(),
            // A back face carries no attachment block at all — there is no field for
            // one, so a transforming Aura cannot be authored rather than misbehaving.
            Self::CardBack { .. } | Self::Token(_) => None,
        }
    }

    /// Whether the object has printed card type `card_type`.
    #[must_use]
    pub fn has_type(&self, card_type: CardType) -> bool {
        self.types().contains(&card_type)
    }

    /// Whether the object has printed subtype `subtype` (case-sensitive, as printed).
    #[must_use]
    pub fn has_subtype(&self, subtype: &str) -> bool {
        self.subtypes().iter().any(|s| s == subtype)
    }

    /// Whether the object has printed keyword ability `keyword` (CR 702). The
    /// *current* keyword set folds in continuous grants; read that through
    /// [`characteristics`](crate::characteristics::characteristics).
    #[must_use]
    pub fn has_keyword(&self, keyword: Keyword) -> bool {
        self.keywords().contains(&keyword)
    }

    /// The display type line, e.g. `"Creature — Goblin"`. Rendered by the one shared
    /// routine [`CardData::type_line`](crate::CardData::type_line) uses, so a token's
    /// type line is built exactly as a card's is.
    #[must_use]
    pub fn type_line(&self) -> String {
        render_type_line(self.supertypes(), self.types(), self.subtypes())
    }

    /// Whether this is a token (CR 111) rather than a card — the same first-class
    /// predicate [`Printed::is_token`] answers, available to code that holds only the
    /// face.
    #[must_use]
    pub fn is_token(&self) -> bool {
        matches!(self, Self::Token(_))
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::card::CardDatabase;

    fn goblin() -> TokenData {
        TokenData {
            name: "Goblin".to_string(),
            types: vec![CardType::Creature],
            subtypes: vec!["Goblin".to_string()],
            colors: vec![Color::Red],
            power: Some(1),
            toughness: Some(1),
            ..TokenData::default()
        }
    }

    #[test]
    fn a_token_face_answers_the_same_questions_a_card_face_does() {
        let db = CardDatabase::bundled().unwrap();
        let token = Printed::Token(Box::new(goblin()));
        let face = token.face(&db).unwrap();
        assert_eq!(face.name(), "Goblin");
        assert_eq!(face.type_line(), "Creature — Goblin");
        assert!(face.has_type(CardType::Creature));
        assert!(face.has_subtype("Goblin"));
        assert_eq!(face.power(), Some(1));
        assert_eq!(face.toughness(), Some(1));
        assert_eq!(face.colors(), [Color::Red]);
        // A token has no mana cost, no supertypes, and no attachment grant.
        assert_eq!(face.mana_cost(), "");
        assert!(face.supertypes().is_empty());
        assert!(face.attachment().is_none());
    }

    #[test]
    fn a_token_has_no_card_and_says_so() {
        let token = Printed::Token(Box::new(goblin()));
        assert!(token.card().is_none(), "a token is not a card (CR 111)");
        assert!(token.is_token());

        let card = Printed::Card {
            card: CardId(7),
            face: Face::Front,
        };
        assert_eq!(card.card(), Some(CardId(7)));
        assert!(!card.is_token());
    }

    #[test]
    fn a_card_face_falls_through_to_the_catalog_and_an_unknown_handle_has_none() {
        let db = CardDatabase::bundled().unwrap();
        let ogre = Printed::from(crate::fixtures::id_in(&db, "onakke_ogre"));
        assert_eq!(ogre.face(&db).unwrap().name(), "Onakke Ogre");
        assert!(Printed::from(CardId(9999)).face(&db).is_none());
    }

    #[test]
    fn token_data_rejects_a_card_identity() {
        // The point of the type: there is no field to put a `functional_id` in, so a
        // token that claimed one would fail to parse rather than enter the catalog's
        // identity space (and, from there, the compatibility report).
        let err = serde_json::from_str::<TokenData>(
            r#"{"name":"Goblin","types":["creature"],"functional_id":"goblin"}"#,
        );
        assert!(err.is_err(), "a token may not carry a functional id");
        // Nor a mana cost, nor a scripted flag.
        assert!(serde_json::from_str::<TokenData>(
            r#"{"name":"G","types":["creature"],"mana_cost":"{1}"}"#
        )
        .is_err());
        assert!(serde_json::from_str::<TokenData>(
            r#"{"name":"G","types":["creature"],"scripted":true}"#
        )
        .is_err());
    }
}
