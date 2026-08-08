//! One class of permanents, asked by everything that names one.
//!
//! Four selectors used to answer the same question — *which permanents?* — and none of
//! them composed: `MassAffects` for a one-shot class,
//! `StaticAffects` for a continuous one, `DestroyAffects` for a sweeper, and
//! [`PermanentCount`](crate::PermanentCount) for a count. Between them, nine of the
//! classes actually authored in the catalog were used by exactly **one** card, and each
//! new phrasing cost a variant plus an arm in every consumer across two crates
//! (issue #824).
//!
//! Nearly every one of them was a *product* of axes the engine already modelled —
//! controller relation, card type, subtype, colour, a keyword, a power bound, whether it
//! is attacking, whether it is a token. [`PermanentCount`] was already that product, and
//! its own doc said why:
//!
//! > Deliberately a small product of independent filters rather than a closed list of
//! > named classes: a count is asked about an open-ended variety of things, and
//! > enumerating each as its own variant would grow the vocabulary once per card.
//!
//! This is that struct, generalised until the other three are expressible in it.

use serde::{Deserialize, Deserializer};

use crate::card::Keyword;
use crate::card_type::CardType;
use crate::mana::Color;
use crate::state::CounterKind;

/// Whose permanents a selector names, relative to the **reading object's controller** —
/// the controller of the spell, ability, or static ability doing the asking.
///
/// Controller-relative rather than by seat, which is what lets one authored card mean
/// "you" from either side of the table. A seat that has lost the game is no longer an
/// opponent (CR 102.1), and [`Self::OpponentsControl`] excludes it everywhere.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ControllerScope {
    /// The reading object's controller — "you control". The default, because it is what
    /// most printed classes say.
    #[default]
    YouControl,
    /// Every opponent of the reading object's controller, and none of their own.
    OpponentsControl,
    /// The player this resolution's most recent targeting effect **named** — the `each
    /// creature **that player** controls` of a spell that hits a player and their board
    /// in one sentence.
    ///
    /// The class counterpart of [`PlayerRef`](crate::PlayerRef)'s `ThatPlayer`, reading the same fact for the
    /// same reason: the choice belongs to the sentence before it. Names nobody in a
    /// resolution that aimed at nothing, which is a card that could not have been
    /// written.
    ThatPlayer,
    /// Every permanent on the battlefield, whoever controls it — the symmetric class a
    /// sweeper names.
    Any,
}

/// How a filter reads the characteristics it asks about.
///
/// The one thing about this predicate that is **not** the same everywhere it is used, and
/// it is a rules fact rather than an inconsistency. A class named by a resolution, a
/// count, or a trigger is asked from outside the CR 613 layer system, so every question
/// is answered from the permanent's *current* characteristics. A class named by a
/// **static ability's own selector** is asked from inside the layer-6/7 walk that
/// produces those characteristics, so it has to read the printed face or it would ask
/// the computation for the answer it is in the middle of producing.
///
/// The asymmetry is the recursion, not a difference of opinion — the same sentence the
/// old `StaticAffects` carried, now stated once instead of per selector. The catalog
/// validator refuses the fields that cannot be answered under [`Self::Printed`] rather
/// than letting them silently read the wrong thing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reading {
    /// From **outside** the layer system: a resolution (CR 611.2c), a count, a
    /// condition, a trigger diff. Types, keywords, power and toughness are the computed
    /// ones (CR 613.1f), so an animated artifact is a creature and a pumped 3/3 has
    /// power 4.
    Computed,
    /// From **inside** the layer walk: a printed static ability selecting the permanents
    /// it modifies. Everything is read off the printed face.
    Printed,
}

/// What a filter needs to know about the object doing the asking.
///
/// Small and `Copy`, because every field is either a handle or a number the caller
/// already has: nothing here is derived, and the predicate derives nothing that outlives
/// one call.
#[derive(Clone, Copy, Debug)]
pub struct FilterContext {
    /// The controller of the object naming the class — what every
    /// [`ControllerScope`] is relative to.
    pub controller: crate::id::PlayerId,
    /// The permanent the class is named *by*, when there is one. `None` for a spell's
    /// own effects and for an emblem, neither of which is a permanent — which is exactly
    /// why [`PermanentFilter::except_this`] and
    /// [`PermanentFilter::with_the_named_card`] then match nothing rather than
    /// everything.
    pub source: Option<crate::id::PermanentId>,
    /// The source's power, read by the caller **before** any cost was paid, for
    /// [`PermanentFilter::below_source_power`]. A source that is already gone took its
    /// power with it, and the class is then empty rather than universal.
    pub source_power: Option<i32>,
    /// The player a targeting effect earlier in this same resolution named, for
    /// [`ControllerScope::ThatPlayer`].
    pub chosen_player: Option<crate::id::PlayerId>,
    /// Which characteristics the questions are answered from.
    pub reading: Reading,
}

/// One or more printed card types, satisfied by **any** of them.
///
/// Authored as a bare string for the ordinary one-type class and as an array for a
/// disjunction — `"creature"` and `["artifact", "enchantment"]`. One key rather than two
/// spellings of two keys, because a card prints one class either way: *destroy all
/// artifacts and enchantments* is one destruction, and a permanent that is both is
/// destroyed once.
fn card_types<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<CardType>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum OneOrMany {
        One(CardType),
        Many(Vec<CardType>),
    }
    Ok(match OneOrMany::deserialize(deserializer)? {
        OneOrMany::One(card_type) => vec![card_type],
        OneOrMany::Many(types) => types,
    })
}

/// A class of permanents, as a product of the axes a printed card actually narrows by.
///
/// Every field absent is "every permanent on the battlefield the scope allows"; each one
/// present narrows further, and they are conjunctive. Growing the vocabulary is adding a
/// field here — once — rather than a variant in four enums and an arm in six exhaustive
/// matches.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PermanentFilter {
    /// Whose permanents. Defaults to the reading object's controller's own.
    #[serde(default)]
    pub scope: ControllerScope,
    /// Printed card types, satisfied by any one of them. Empty names every type.
    #[serde(default, deserialize_with = "card_types")]
    pub card_type: Vec<CardType>,
    /// A printed subtype the permanent must have — the `Elves` of a lord, the `Dragons`
    /// of a mass pump. Absent names every subtype.
    #[serde(default)]
    pub subtype: Option<String>,
    /// A printed colour (CR 105.2) the permanent must be. Absent names every colour,
    /// colourless included.
    ///
    /// Printed rather than computed even under [`Reading::Computed`], because CR 613
    /// layer 5 is not modelled for permanents on the battlefield: printed colour *is*
    /// current colour here, exactly as it is in the blocking restrictions that name one.
    #[serde(default)]
    pub color: Option<Color>,
    /// A keyword the permanent must have — the `with defender` of a class that names
    /// one.
    ///
    /// Read through the computed keyword set under [`Reading::Computed`], so a *granted*
    /// defender is in the class; off the printed face under [`Reading::Printed`], where
    /// asking for the computed set would not terminate.
    #[serde(default)]
    pub keyword: Option<Keyword>,
    /// A keyword the permanent must **not** have — the `without flying` of an effect
    /// that clears the ground. Read exactly as [`Self::keyword`] is.
    #[serde(default)]
    pub without_keyword: Option<Keyword>,
    /// The least power the permanent may have — `with power 4 or greater`.
    ///
    /// Read through the **computed** characteristics, which is the only reading a printed
    /// card means, and therefore refusable under [`Reading::Printed`]: the catalog
    /// validator rejects it in a static ability's selector and in an attachment's count
    /// rather than letting it recurse
    /// ([`Violation::PowerInStaticCondition`](crate::Violation)).
    #[serde(default)]
    pub min_power: Option<i32>,
    /// The greatest toughness the permanent may have — the bound the old selectors could
    /// not express at all. Read and restricted exactly as [`Self::min_power`] is.
    #[serde(default)]
    pub max_toughness: Option<i32>,
    /// Power strictly less than the **source's** — the `creatures you control with power
    /// less than Lena's power` of a sacrifice that protects the small.
    ///
    /// A flag rather than a value, because the number it compares against is not knowable
    /// when the card is authored. A source that has left — sacrificed to its own cost,
    /// which is exactly what Lena does — took its power with it, and the class is then
    /// empty rather than everything.
    #[serde(default)]
    pub below_source_power: bool,
    /// Only permanents currently **attacking** (CR 508.1a) — the class a combat pump
    /// names. Empty outside combat, which makes such a spell a legal but pointless
    /// main-phase cast rather than an uncastable one.
    #[serde(default)]
    pub attacking: bool,
    /// Whether the permanent must be a token (`true`) or must not be (`false`); absent
    /// takes both.
    ///
    /// Read off what the permanent *is* rather than inferred: a token has no card at all
    /// (CR 111), which the state already records.
    #[serde(default)]
    pub token: Option<bool>,
    /// At least one counter of this kind on the permanent — the `permanents with
    /// **phylactery counters** on them` a Lich's life depends on.
    ///
    /// The one field here that is not a characteristic: a counter is not produced by any
    /// layer, so unlike [`Self::min_power`] it can be asked from anywhere, static
    /// selectors included.
    #[serde(default)]
    pub with_counter: Option<CounterKind>,
    /// Only the permanent whose printed card the **source named as it entered**
    /// (CR 614.12) — the `with the chosen name` of a card that asks for one.
    ///
    /// Compares card identity, not a string: two printings of one functional card share a
    /// [`CardId`](crate::CardId) and nothing else does, and a token has no card to bear a
    /// name anyone named. A source that named nothing matches nothing.
    #[serde(default)]
    pub with_the_named_card: bool,
    /// Exclude the source itself — the `other` in "other Elves you control".
    ///
    /// Compares the specific object: a [`PermanentId`](crate::PermanentId) is minted
    /// fresh on every battlefield entry, so two copies of one lord do pump each other. An
    /// object with no source permanent excludes nothing.
    #[serde(default)]
    pub except_this: bool,
}

impl PermanentFilter {
    /// The everything-the-controller-controls filter, for a caller building one in code.
    #[must_use]
    pub fn you_control() -> Self {
        Self::default()
    }

    /// Narrow to one printed card type.
    #[must_use]
    pub fn of_type(mut self, card_type: CardType) -> Self {
        self.card_type = vec![card_type];
        self
    }

    /// Whether this filter asks a question that cannot be answered from inside the layer
    /// system — the fields the catalog validator refuses in a static ability's selector
    /// and in an attachment's count.
    ///
    /// Stated here, beside the fields, so the rule and the reason live together: a bound
    /// on a computed power or toughness asked from within the computation of a
    /// permanent's characteristics would not terminate.
    #[must_use]
    pub fn reads_computed_power(&self) -> bool {
        self.min_power.is_some() || self.max_toughness.is_some() || self.below_source_power
    }
}
