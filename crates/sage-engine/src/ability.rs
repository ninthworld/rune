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

use crate::card::{CombatRestriction, DamageCharacteristic, Keyword, RuleModification};
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
        /// Whether it may be activated **only once each turn** (CR 602.5f) — the
        /// `Activate only once each turn.` a card prints under its cost.
        ///
        /// An authored field for [`Self::Activated::timing`]'s reason: it is a line of
        /// printed text on one particular ability rather than a rule about a kind of
        /// ability, so nothing about the cost or the effects could derive it. The
        /// allowance is per **permanent and ability**, recorded on the state and cleared
        /// at the turn boundary — a creature that leaves and returns is a new object with
        /// a fresh one, exactly as CR 606.3 says of a planeswalker's loyalty.
        #[serde(default)]
        once_each_turn: bool,
        /// **When** it may be activated (CR 602.5d). Defaults to
        /// [`ActivationTiming::AnyTime`], which is every ability in the catalog but the
        /// one that prints `Activate only as a sorcery.`
        ///
        /// An authored field rather than a derived predicate, unlike the loyalty
        /// (CR 606.3) and equip (CR 702.6b) timings beside it. Those two are rules
        /// *about a kind of ability* — an ability that spends loyalty is a loyalty
        /// ability whatever it does — so deriving them from the cost or the effect is
        /// exact. This one is a line of printed text on one particular ability, and
        /// nothing about that ability's cost or effect implies it.
        #[serde(default)]
        timing: ActivationTiming,
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
    /// Applied by the CR 614 replacement layer at the battlefield-entry seam, not as a
    /// post-action pipeline stage. Deserialized as `{"type":"enters_tapped"}`.
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
        /// How many counters of that kind the permanent enters with. Ignored when
        /// [`from_announced_x`](Self::EntersWithCounters::from_announced_x) is set, which
        /// is where the number comes from then — and omitted on the card that sets it,
        /// which prints no number at all.
        #[serde(default)]
        count: u32,
        /// Whether the number is the **X its controller announced** as the spell was cast
        /// (CR 601.2b) — `This creature enters with X +1/+1 counters on it`.
        ///
        /// A flag rather than a general amount, because the only thing a printed card
        /// puts here is its own X: the counters are placed as the permanent enters, and a
        /// number read off the board at that moment would be read before the permanent is
        /// there to be counted among.
        #[serde(default)]
        from_announced_x: bool,
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
    /// A choice made **as this permanent enters** (CR 614.12): its controller names a
    /// *card*, and the answer is kept on the permanent for as long as it is on the
    /// battlefield — the "chosen name" its other abilities read.
    ///
    /// [`Self::EntersChoosingColor`]'s sibling, riding the same seam for the same reason:
    /// the card waits off the battlefield while the question is owed, so there is no
    /// instant at which the permanent is there without an answer and no window in which
    /// a player could respond to one.
    ///
    /// What differs is the answer set, and it is the whole of the project's legal
    /// posture on naming a card. A colour is one of five and needs nothing derived; a
    /// card name is chosen from the **catalog** ([`named_card_candidates`](crate::named_card_candidates)),
    /// so what is recorded is a [`CardId`](crate::CardId) — a handle to a card SAGE has
    /// defined — and never a string a player typed. SAGE ships no card name it has not
    /// already written down, and this variant is why that stays true of a game in
    /// progress as well as of the repository.
    ///
    /// The answer lives on [`Permanent::named_card`](crate::Permanent), not here.
    /// Deserialized as `{"type":"enters_naming_card","class":"nonbasic_land"}`.
    EntersNamingCard {
        /// Which cards may be named — the "nonbasic land" a card puts between "name a"
        /// and "card".
        class: crate::choice::NamedCardClass,
    },
    /// A **characteristic-defining ability** (CR 604.3) that *sets* this permanent's
    /// power to the number of cards `count_of` names — `Enigma Drake's power is equal to
    /// the number of instant and sorcery cards in your graveyard.`
    ///
    /// **Not an effect, and the difference is the whole variant.** Every other amount in
    /// the IR is taken once, where a resolution reaches it (CR 608.2), and the number
    /// that comes out is fixed for good. This one is re-derived on *every read* of the
    /// permanent's power: a card put into the graveyard changes it with nothing going on
    /// the stack, no event in between, and no window in which the old number is still
    /// showing. That is what CR 604.3 says a characteristic-defining ability is, and it
    /// is why the number lives in the layer system rather than in an effect.
    ///
    /// It applies in CR 613 **layer 7a**, ahead of every other power/toughness layer, so
    /// it *replaces* the printed power and everything else piles on top of the result: a
    /// `+1/+1` counter (7c) still adds one, an anthem (7c) still adds its own, and a
    /// later effect that sets base power would still overrule it (7b). Printed power is
    /// authored as `0` on such a card — the `*` in the corner is this ability, not a
    /// number — and is never what a reader sees.
    ///
    /// Only power is definable. The cards that define toughness the same way, and the
    /// ones whose defined characteristic is a colour or a type, each add their own
    /// variant when they arrive; a single "defines a characteristic" variant carrying a
    /// layer number would let a card be authored that defines its power in layer 4.
    ///
    /// Deserialized as
    /// `{"type":"defined_power","count_of":{"filter":{"kind":"instant_or_sorcery"}}}`.
    DefinedPower {
        /// Which cards, in whose graveyards, the power is equal to a count of.
        count_of: GraveyardCount,
    },
    /// A **copy effect** fixed as this permanent enters (CR 614.12 + CR 707): its
    /// controller names a permanent, and from then on either this permanent or the one it
    /// is attached to has that permanent's copiable values (CR 613 layer 1).
    ///
    /// Two printed shapes ride this one variant, and [`subject`](Self::EntersAsCopy::subject)
    /// is the whole difference between them:
    ///
    /// - `You may have this creature enter as a copy of a creature you control` — the
    ///   entering permanent *is* the copy (CR 707.5). It becomes one **as** it enters and
    ///   not afterwards, so its enters-the-battlefield triggers are the copied ones and a
    ///   0/0 that copies a 2/2 was never a 0/0 on the battlefield.
    /// - `As this Aura enters, choose a creature. Enchanted creature is a copy of the
    ///   chosen creature` — the *host* is the copy, for exactly as long as the Aura is
    ///   attached to it (CR 707.2c: a copy effect from a static ability determines its
    ///   copiable values only when it first starts to apply, which is why the answer is
    ///   snapshotted here rather than re-read).
    ///
    /// Like [`Self::EntersChoosingColor`] it is a **question**, not a modification, so the
    /// CR 614 replacement layer does not collect it and there is nothing to order it
    /// against (ADR 0019): the card waits off the battlefield until the answer comes back,
    /// and the permanent that then enters already carries its copiable values. The answer
    /// lives on [`Permanent::copied`](crate::Permanent). Deserialized as
    /// `{"type":"enters_as_copy","of":"creature_you_control","optional":true}`.
    EntersAsCopy {
        /// Which permanents may be named.
        of: crate::copy::CopyClass,
        /// What becomes the copy — this permanent, or the one it is attached to.
        #[serde(default)]
        subject: crate::copy::CopySubject,
        /// Whether the controller may decline — the `You may have …` of every printed
        /// `enters as a copy`. A mandatory choice with no legal answer simply chooses
        /// nothing, so this is about the *decision*, not about the empty board.
        #[serde(default)]
        optional: bool,
    },
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
    /// `You have no maximum hand size.`, `You may play lands from your graveyard.`
    ///
    /// A separate variant rather than a widening of
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
    /// A continuous ability that changes **what a spell costs to cast** (CR 601.2f) —
    /// `Creature spells you cast with power 4 or greater cost {2} less to cast.`
    ///
    /// A third continuous variant beside [`Self::Static`] and [`Self::PlayerStatic`],
    /// and separate from both for the reason they are separate from each other: its
    /// subject is a **spell**, which is neither a permanent nor a player. A
    /// [`StaticAffects`] names a class of permanents and a [`StaticModification`] names
    /// a CR 613 layer; a cost modification is not a layer at all — it applies while a
    /// spell is being cast, before the object it produces exists — so a single variant
    /// carrying both vocabularies could express nothing but nonsense.
    ///
    /// Read where the question is asked ([`crate::total_cast_cost`]) rather than applied
    /// anywhere, exactly as the other two are: it takes effect the instant its source is
    /// on the battlefield and stops the instant it leaves, with nothing stored and
    /// nothing to prune (ADR 0005 §1). Every road that reads a cast's cost — the offer,
    /// the payment, and the charge — goes through that one function, so a spell is never
    /// advertised at one price and charged another.
    ///
    /// The caster is always the source's **controller** — the "you cast" every printed
    /// ability of this shape says — so there is no selector to author and none to get
    /// wrong. A tax on *another* player's spells is a different sentence and would name
    /// the scope here.
    ///
    /// Deserialized as
    /// `{"type":"cost_modifier","spells":{"creature":{"min_power":4}},"modification":{"kind":"reduce","generic":2}}`.
    CostModifier {
        /// Which of its controller's spells the modification applies to — the same
        /// closed class vocabulary a cast trigger watches.
        spells: ObservedSpell,
        /// How much generic mana it adds or takes off (CR 601.2f).
        modification: CostModification,
    },
}

/// What an [`Ability::PlayerStatic`] does to its controller.
///
/// A closed, plain-data enum of the rules a permanent can change *about a person*. It
/// grows by adding variants — a card that *raises* a maximum hand size rather than
/// removing it is a different thing to say, and would say it here.
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
    /// CR 601.2b: the controller may **cast spells from their hand without paying their
    /// mana costs** — Omniscience.
    ///
    /// An *alternative cost* rather than a reduction, and the difference is why this is a
    /// permission rather than a [`CostModifier`](Ability::CostModifier) of minus
    /// everything: a reduction adjusts the mana component and cannot take a coloured pip
    /// off, while this replaces it outright. It is read where every cost is read
    /// ([`total_cast_cost`](crate::total_cast_cost)), so the offer, the payment search,
    /// the charge, and the view agree by construction.
    ///
    /// **The mana component only.** An additional cost the card names (a discard, a
    /// sacrifice) is still paid, because CR 601.2b replaces the mana cost and nothing
    /// else. `{X}` is `0` when nothing pays for it (CR 107.3b).
    ///
    /// Scoped to the **hand** because that is what the card says. Casting without paying
    /// from a graveyard, from exile, or off a library is a different permission each
    /// time, and each names its own zone.
    CastFromHandWithoutPaying,
    /// CR 305.9 / CR 116.2a: the controller may **play lands from their graveyard**,
    /// as though those cards were in their hand.
    ///
    /// A *permission about a zone*, and the second of the two independent primitives
    /// this rule needs — the other being that a land is **played**, never cast, so it
    /// reaches the battlefield through [`Action::PlayLand`](crate::Action) and never
    /// through the stack. Nothing else about the land play changes: it is still one per
    /// turn (CR 305.2), still the active player's, still at sorcery speed, because those
    /// gates are asked of the *play* rather than of the zone it came from.
    ///
    /// Not the same thing as [`Effect::AllowCastingFromGraveyard`] and deliberately not
    /// folded into it. That one is a permission granted *for a turn* by a resolved
    /// effect, so it is recorded in [`GameState`](crate::GameState) with the turn it was
    /// granted on; this one is a continuous ability of a permanent, so it is read where
    /// the question is asked and lasts exactly as long as its source is on the
    /// battlefield — and a permission to *cast* could never authorise a land, which is
    /// not cast at all.
    ///
    /// Read by [`plays_lands_from_graveyard`](crate::plays_lands_from_graveyard).
    PlayLandsFromGraveyard,
}

/// When an activated ability may be activated (CR 602.5d).
///
/// A two-state type rather than a bool for the reason the rest of the IR prefers one: a
/// field called `sorcery_speed` reads as a fact about the ability, while the question is
/// *when*, and a third timing (`Activate only during combat`) would then be a second
/// bool that could disagree with the first.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivationTiming {
    /// Any time its controller has priority (CR 602.2a) — the default, and what every
    /// activated ability says unless it says otherwise.
    #[default]
    AnyTime,
    /// Only when its controller could cast a sorcery: their turn, a main phase, an empty
    /// stack. The `Activate only as a sorcery.` of a printed card, enforced by the same
    /// single expression of "sorcery speed" the loyalty and equip gates share, so the
    /// three cannot disagree about when that is.
    SorcerySpeed,
}

/// Whether an ability is restricted to **sorcery speed** by its printed text
/// (CR 602.5d) — whether it declares [`ActivationTiming::SorcerySpeed`].
///
/// The counterpart of [`is_loyalty_ability`] and [`is_equip_ability`] for the timing
/// that is *authored* rather than derived. It is a predicate all the same, so the offer
/// and the apply-time re-derivation ask one question and cannot drift.
#[must_use]
pub fn is_sorcery_speed_ability(ability: &Ability) -> bool {
    matches!(
        ability,
        Ability::Activated {
            timing: ActivationTiming::SorcerySpeed,
            ..
        }
    )
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
/// or triggered ability that returns its own card from there
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
/// **not** read off a permanent ([`crate::valid_actions`] for an activation,
/// [`crate::collect_triggers`] for a trigger), that it *is* read off a card sitting in a
/// graveyard, and — re-derived rather than trusted — that an activation naming a
/// graveyard card is legal at all ([`crate::apply_action`]). A graveyard ability is never
/// a mana ability: [`is_mana_ability`] requires every effect to be a mana verb and this
/// one is not, so no exclusion has to be written there.
///
/// The search is over the whole effect **tree**, not the top-level list, because the
/// return is frequently the payoff of an optional cost: `you may pay {R}. If you do,
/// return this card from your graveyard to your hand` says it inside an [`Effect::May`],
/// and an ability that functions in a graveyard only when the player pays still functions
/// in a graveyard.
///
/// A trigger that watches its **own source dying** is the one exception, and it is not a
/// hedge. Such an ability functions from the battlefield — that is where its source was
/// when the event happened, and CR 603.6c is the rule that lets it fire on the way out —
/// so it belongs to the battlefield pass however its effects then reach the card the
/// permanent became. Classifying it by its effect alone would file it under the graveyard
/// pass, which reads printed abilities off cards already sitting there and would never
/// have seen a permanent die at all.
#[must_use]
pub fn is_graveyard_ability(ability: &Ability) -> bool {
    match ability {
        Ability::Triggered {
            event: TriggerCondition::SelfDies,
            ..
        } => false,
        Ability::Activated { effects, .. } | Ability::Triggered { effects, .. } => {
            returns_self_from_graveyard(effects)
        }
        _ => false,
    }
}

/// Whether `effects`, or anything nested inside them, is an
/// [`Effect::ReturnSelfFromGraveyard`].
///
/// Walks the two wrappers that carry effect lists — the optional [`Effect::May`] and the
/// branching [`Effect::Conditional`] — so where the return sits in the tree never changes
/// the answer to "where does this ability function".
fn returns_self_from_graveyard(effects: &[Effect]) -> bool {
    effects.iter().any(|effect| match effect {
        Effect::ReturnSelfFromGraveyard { .. } => true,
        Effect::May { effects, .. } => returns_self_from_graveyard(effects),
        Effect::Conditional {
            then, otherwise, ..
        } => returns_self_from_graveyard(then) || returns_self_from_graveyard(otherwise),
        _ => false,
    })
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
