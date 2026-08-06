//! The card-effect IR: a small, closed, serde-friendly vocabulary of abilities,
//! costs, effects, and trigger conditions.
//!
//! Abilities are **data**, carried on [`crate::CardData`] and interpreted by pure
//! functions over [`crate::GameState`] (see `crate::apply_action`). Nothing here
//! is a closure or a listener: a triggered ability's condition is a value matched
//! by a pure predicate against a before/after diff, honoring the engine's
//! pull-based, no-observer rule (`crates/sage-engine/AGENTS.md`).
//!
//! Cards the closed IR cannot express fall back to the code table in
//! [`crate::scripted`]; see `docs/decisions/0003-card-effect-ir-hybrid.md`.

use serde::Deserialize;

use crate::card::{CombatRestriction, Keyword};
use crate::card_type::CardType;
use crate::id::{CardInstanceId, PermanentId, PlayerId};
use crate::mana::Color;
use crate::stack::StackId;
use crate::state::CounterKind;
use crate::token::TokenData;

mod cost;
mod effect;
mod selector;
mod static_ability;
mod target;
mod trigger;

pub use cost::*;
pub use effect::*;
pub use selector::*;
pub use static_ability::*;
pub use target::*;
pub use trigger::*;

/// The default `count` of an [`Effect::CreateToken`]: one token, the overwhelmingly
/// common case ("create a 1/1 white Soldier creature token").
fn one() -> u8 {
    1
}

/// One ability of a card.
///
/// The set is deliberately small and grows by adding variants (static/keyword
/// abilities arrive later). Deserialized with an internal `type` tag, e.g.
/// `{"type": "activated", ...}`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Ability {
    /// An activated ability: pay its costs to produce its effects (e.g. a land's
    /// `{T}: Add {G}`).
    Activated {
        /// Costs paid to activate, all of which must be paid.
        cost: Vec<Cost>,
        /// Effects produced when the ability resolves (or immediately, for a
        /// mana ability — see [`is_mana_ability`]).
        effects: Vec<Effect>,
    },
    /// A triggered ability: when its condition is met, its effects go on the
    /// stack (e.g. `When this enters the battlefield, draw a card.`).
    Triggered {
        /// The condition that causes this ability to trigger.
        event: TriggerCondition,
        /// Effects produced when the triggered ability resolves.
        effects: Vec<Effect>,
    },
    /// A **self-replacement** (CR 614.1c): this permanent enters the battlefield
    /// **tapped** (e.g. a tapped dual land). Unlike a triggered ability it changes
    /// nothing after the fact — it modifies the enters-the-battlefield event itself,
    /// so the permanent is tapped the instant it is on the battlefield, before any
    /// state-based action or enters-the-battlefield trigger is observed (CR 614.12).
    /// Applied at the battlefield-entry seam ([`crate::card::apply_enters_replacements`]),
    /// not as a post-action pipeline stage. Deserialized as `{"type":"enters_tapped"}`.
    EntersTapped,
    /// A **self-replacement** (CR 614.1c): this permanent enters the battlefield
    /// with `count` counters of `counter` already on it (CR 614.12) — e.g. a 0/0 that
    /// enters with two `+1/+1` counters. The counters are part of *entering*: they are
    /// present before state-based actions run, so such a creature is never a 0/0 on the
    /// battlefield and survives the CR 704.5f toughness check. Like [`Self::EntersTapped`]
    /// it is applied at the entry seam, and the co-entering ETB trigger observes the
    /// replaced state (CR 614.12). Deserialized as
    /// `{"type":"enters_with_counters","counter":"plus_one_plus_one","count":2}`.
    EntersWithCounters {
        /// The kind of counter placed as the permanent enters. Named `counter` on
        /// the wire because the enum already reserves the `type` tag for its own
        /// discriminant.
        counter: CounterKind,
        /// How many counters of that kind the permanent enters with.
        count: u32,
    },
    /// A choice made **as this permanent enters** (CR 614.12): its controller names one
    /// of the five colors, and the answer is kept on the permanent for as long as it is
    /// on the battlefield — the "chosen color" every later ability of the card reads.
    ///
    /// It is not an enters-the-battlefield *trigger*, and the difference is the whole
    /// variant. A trigger goes on the stack after the permanent has arrived, so there
    /// would be a window in which the permanent sat on the battlefield with no answer
    /// recorded and any player could respond to it. This is part of the arrival: the
    /// card waits *off* the battlefield while its controller answers
    /// ([`ColorOutcome::RecordOnEntry`](crate::ColorOutcome)), exactly as a spell's card
    /// waits off the stack while a mid-resolution choice is owed, and the permanent that
    /// then enters already carries its colour.
    ///
    /// The answer lives on [`Permanent::chosen_color`](crate::Permanent), not here: this
    /// variant is the card's *declaration* that a colour is chosen, in the same way
    /// [`Self::EntersTapped`] is a declaration about the entry event rather than a record
    /// of one. Deserialized as `{"type":"enters_choosing_color"}`.
    EntersChoosingColor,
    /// A **static ability** (CR 604.3): a continuous effect that applies for as long
    /// as this permanent is on the battlefield, with nothing ever put on the stack —
    /// an anthem (`Creatures you control get +1/+1.`) or a lord (`Other Elves you
    /// control get +1/+1.`).
    ///
    /// Unlike every other variant here, this one is never *applied* by an action.
    /// It is read by [`characteristics`](crate::characteristics::characteristics)
    /// while computing a permanent's current values, so it takes effect the instant
    /// its source is on the battlefield and stops the instant it leaves — derived,
    /// never stored, exactly as an Aura's grant is (ADR 0005 §1). Nothing enters
    /// `GameState::static_effects`, so there is nothing to prune and no way for the
    /// effect to outlive its source.
    ///
    /// Deserialized as
    /// `{"type":"static","affects":{"scope":"creatures_you_control","subtype":"Elf","except_this":true},"modification":{"kind":"power_toughness","power":1,"toughness":1}}`.
    Static {
        /// Which permanents the continuous effect applies to.
        affects: StaticAffects,
        /// What it does to them, and therefore which CR 613 layer it applies in.
        modification: StaticModification,
        /// What has to be true for it to be in force at all — the `as long as …` of a
        /// conditional continuous ability. Absent is unconditional, which is what every
        /// anthem and lord says. Re-asked on every read, so the modification appears and
        /// disappears with the condition and there is nothing to prune.
        #[serde(default)]
        condition: Option<StaticCondition>,
    },
    /// A continuous ability whose subject is a **player** rather than a permanent —
    /// `You have no maximum hand size.`
    ///
    /// The first of its kind, and a separate variant rather than a widening of
    /// [`Self::Static`] because the two share nothing but the word "continuous".
    /// A [`StaticAffects`] names a class of permanents and a [`StaticModification`]
    /// names a CR 613 layer; neither has anything to say about a player, and a single
    /// variant carrying both vocabularies would be able to express `{"affects":
    /// "source", "modification": "no_maximum_hand_size"}` — nonsense the loader would
    /// then have to reject at runtime instead of the type rejecting it outright.
    ///
    /// The subject is always the source's **controller**: every printed ability of this
    /// shape says "you", so there is no selector to author and none to get wrong.
    ///
    /// Like [`Self::Static`] it is read where the question is asked rather than applied
    /// anywhere — see [`maximum_hand_size`](crate::maximum_hand_size) — so it takes
    /// effect the instant its source is on the battlefield and stops the instant it
    /// leaves, with nothing stored and nothing to prune (ADR 0005 §1).
    ///
    /// Deserialized as `{"type":"player_static","modification":{"kind":"no_maximum_hand_size"}}`.
    PlayerStatic {
        /// What it does to that player.
        modification: PlayerModification,
    },
}

/// What an [`Ability::PlayerStatic`] does to its controller.
///
/// A closed, plain-data enum with one variant, which is the one M19 prints. It grows by
/// adding variants — a card that *raises* a maximum hand size rather than removing it is
/// a different thing to say, and would say it here.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PlayerModification {
    /// CR 402.2: the controller has **no** maximum hand size, so the cleanup-step
    /// discard of CR 514.1 never applies to them.
    ///
    /// Deliberately not "a very large maximum". No maximum is a different state from a
    /// big number: a sentinel would compare, print, and project as a number nobody
    /// printed, and every call site would have to know which number meant "none".
    NoMaximumHandSize,
}

/// Whether an ability is a **loyalty ability** (CR 606.1): an activated ability whose
/// cost includes a loyalty symbol ([`Cost::Loyalty`]).
///
/// The predicate the two timing rules of CR 606.3 hang off — sorcery speed, and one
/// per planeswalker per turn. Derived from the cost, never stored and never a flag on
/// the card, so an ability cannot claim to be one without paying like one.
///
/// A loyalty ability is never a mana ability — CR 605.1a says so outright, and
/// [`is_mana_ability`] enforces it by asking this predicate first. It has to: a
/// planeswalker ability whose whole effect list adds mana is a real printed card
/// (`Sarkhan, Fireblood`), so "no loyalty ability could pass the effect test" was never
/// a safe thing to rely on. The loyalty gates below are keyed on this predicate rather
/// than on "uses the stack", so they apply either way.
#[must_use]
pub fn is_loyalty_ability(ability: &Ability) -> bool {
    matches!(
        ability,
        Ability::Activated { cost, .. } if cost.iter().any(|c| matches!(c, Cost::Loyalty { .. }))
    )
}

/// Whether an ability is an **equip ability** (CR 702.6a): an activated ability that
/// attaches its source to the permanent it targets.
///
/// The predicate CR 702.6b's timing rule hangs off — equip is activated only when its
/// controller could cast a sorcery — and the exact counterpart of [`is_loyalty_ability`]:
/// derived from what the ability *does*, never stored and never a flag on the card, so an
/// ability cannot equip without being bound by equip's timing.
///
/// An equip ability is never a mana ability: [`is_mana_ability`] requires every effect to
/// be a mana verb and [`Effect::Attach`] is not one, so no exclusion has to be written
/// here.
#[must_use]
pub fn is_equip_ability(ability: &Ability) -> bool {
    matches!(
        ability,
        Ability::Activated { effects, .. } if effects.iter().any(|e| matches!(e, Effect::Attach { .. }))
    )
}

/// Whether an ability **functions from its owner's graveyard** (CR 113.6): an activated
/// ability that returns its own card from there
/// ([`Effect::ReturnSelfFromGraveyard`]).
///
/// Derived from what the ability *does*, never stored and never a flag on the card — the
/// shape [`is_loyalty_ability`] and [`is_equip_ability`] use, and here it is more than a
/// convention. An ability that moves its own card out of a graveyard could function
/// nowhere else: on the battlefield its source is a permanent and there is no card in a
/// graveyard for it to move. So "where it works from" is not a second fact an author
/// could get out of step with the text; it is the text.
///
/// This is the predicate the whole zone seam hangs off. It decides that the ability is
/// **not** offered on a permanent ([`crate::valid_actions`]), that it *is* offered on a
/// card sitting in its controller's graveyard, and — re-derived rather than trusted — that
/// an activation naming a graveyard card is legal at all
/// ([`crate::apply_action`]). A graveyard ability is never a mana ability:
/// [`is_mana_ability`] requires every effect to be a mana verb and this one is not, so no
/// exclusion has to be written there.
#[must_use]
pub fn is_graveyard_ability(ability: &Ability) -> bool {
    matches!(
        ability,
        Ability::Activated { effects, .. }
            if effects
                .iter()
                .any(|e| matches!(e, Effect::ReturnSelfFromGraveyard { .. }))
    )
}

/// Whether an ability is a mana ability (CR 605.1a, simplified): an activated
/// ability whose every effect adds mana, **and which is not a loyalty ability**. Mana
/// abilities resolve immediately and do not use the stack (see `crate::apply_action`).
/// Derived, never stored.
///
/// The loyalty exclusion is CR 605.1a's own, and it is not hypothetical: a
/// planeswalker's `+1: Add two mana in any combination of colors` adds nothing but
/// mana, and without this clause it would resolve immediately, off the stack, with no
/// window for anyone to respond — a loyalty activation nobody could see coming.
#[must_use]
pub fn is_mana_ability(ability: &Ability) -> bool {
    matches!(
        ability,
        Ability::Activated { effects, .. }
            if !is_loyalty_ability(ability)
                && !effects.is_empty()
                && effects.iter().all(|e| matches!(
                    e,
                    Effect::AddMana { .. }
                        | Effect::AddColorlessMana { .. }
                        | Effect::AddRestrictedMana { .. }
                        | Effect::AddManaAnyColor { .. }
                ))
    )
}

/// Whether paying `ability`'s activation cost **taps its source** — the `{T}` in
/// `{T}: Add {G}` (CR 602.2a).
///
/// Exposed for the same reason [`is_mana_ability`] is: a client offers a gesture over an
/// activation it is told about, and *what that gesture does to the card* is a rules
/// question. A land tapped for mana turns sideways and a mana rock that sacrifices itself
/// does not, and no presentation can tell those apart without reading the cost — so the
/// server states it per candidate (`docs/protocol.md`) and the client draws what it is
/// told. Derived from the cost alone; nothing is applied.
#[must_use]
pub fn activation_taps(ability: &Ability) -> bool {
    matches!(
        ability,
        Ability::Activated { cost, .. } if cost.iter().any(|c| matches!(c, Cost::Tap))
    )
}

/// Whether `ability` is one an [`Emblem`](crate::Emblem) may carry (CR 114.1–114.4).
///
/// An emblem has no characteristics but its abilities, is in no zone, and is never an
/// object a player can act on — so only the two ability kinds that need neither an
/// activation nor an entry event apply to it. An activated ability would have to be
/// activated from somewhere, and an enters-the-battlefield self-replacement would have
/// to replace an entry that never happens.
///
/// Enforced by the catalog validator at authoring time, so a card that writes one of
/// the others fails the build rather than creating an emblem with a dead ability.
#[must_use]
pub fn is_emblem_ability(ability: &Ability) -> bool {
    matches!(ability, Ability::Static { .. } | Ability::Triggered { .. })
}

#[cfg(test)]
mod tests;
