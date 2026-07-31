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
use crate::id::{CardInstanceId, PermanentId, PlayerId};
use crate::mana::Color;
use crate::stack::StackId;
use crate::state::CounterKind;

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
    },
}

/// Which permanents a printed [`Ability::Static`] continuously modifies.
///
/// A closed, authored selector: it names a *class*, and is evaluated against each
/// permanent on demand relative to the ability's own source. Deliberately small —
/// it covers the anthem and lord shapes and grows by adding variants when a card
/// needs one ("creatures your opponents control", "permanents you control").
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "scope", rename_all = "snake_case")]
pub enum StaticAffects {
    /// Creatures controlled by the source's controller — "creatures you control".
    CreaturesYouControl {
        /// Restrict to creatures whose subtypes include this one, which is what
        /// makes a lord a lord (`Other **Elves** you control`). Absent means every
        /// creature its controller controls.
        #[serde(default)]
        subtype: Option<String>,
        /// Exclude the source itself — the "other" in "other Elves you control".
        /// A lord that pumped itself would be a different card.
        #[serde(default)]
        except_this: bool,
    },
}

/// What a printed [`Ability::Static`] does to the permanents it affects. The
/// variant fixes the CR 613 layer, exactly as the runtime
/// [`Modification`](crate::Modification) it maps to does.
///
/// This is the *authored* shape and is deliberately separate from the runtime enum,
/// the same seam [`TargetSpec`] keeps from [`Target`]: the JSON a card is written in
/// must not shift because an internal representation changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StaticModification {
    /// CR 613 **layer 7c**: add the given signed amounts to power and toughness.
    PowerToughness {
        /// Amount added to power; negative subtracts.
        power: i32,
        /// Amount added to toughness; negative subtracts.
        toughness: i32,
    },
    /// CR 613 **layer 6** (CR 613.1f): grant a keyword ability. Redundant grants are
    /// idempotent (CR 702.2c), so an anthem granting flying to a creature that already
    /// flies changes nothing.
    GrantKeyword {
        /// The keyword granted for as long as the source is on the battlefield.
        keyword: Keyword,
    },
}

impl StaticModification {
    /// The runtime [`Modification`](crate::Modification) this authored shape denotes.
    #[must_use]
    pub fn to_modification(self) -> crate::Modification {
        match self {
            StaticModification::PowerToughness { power, toughness } => {
                crate::Modification::PowerToughness { power, toughness }
            }
            StaticModification::GrantKeyword { keyword } => {
                crate::Modification::GrantKeyword(keyword)
            }
        }
    }
}

/// A cost paid to activate an ability.
///
/// Deserialized with an internal `kind` tag, e.g. `{"kind": "tap"}`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Cost {
    /// Tap the source permanent (`{T}`). Payable only while it is untapped.
    Tap,
    /// Pay mana (CR 118): the activation cost written in the same curly-brace
    /// notation [`crate::CardData::mana_cost`] uses, e.g. `{"kind":"mana","mana":"{1}{R}"}`.
    ///
    /// Paid from the activating player's mana pool through the one
    /// [`ManaPool::pay`](crate::ManaPool::pay) seam a spell's cost uses, so an
    /// activation and a cast can never disagree about what a cost string means. The
    /// cost is parsed on demand rather than stored pre-parsed: the authored card data
    /// stays a string, exactly as a card is written.
    Mana {
        /// The mana cost in curly-brace notation. Named `mana` on the wire because
        /// the enum already reserves the `kind` tag for its own discriminant.
        mana: String,
    },
}

/// A single effect an ability (or spell) produces.
///
/// Deserialized with an internal `kind` tag, e.g.
/// `{"kind": "add_mana", "color": "green", "amount": 1}`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Effect {
    /// Add mana to the controller's mana pool.
    AddMana {
        /// The color of mana produced.
        color: Color,
        /// How much mana of that color is produced.
        amount: u8,
    },
    /// Add **colorless** mana (`{C}`) to the controller's mana pool — the mana-rock
    /// verb (e.g. an artifact's `{T}: Add {C}`).
    ///
    /// Colorless is not one of the five [`Color`]s (CR 105.1), so it is a distinct
    /// effect rather than an [`Effect::AddMana`] over a sixth color: keeping it out of
    /// [`Color`] stops a colorless value from ever standing in for a card's color
    /// ([`crate::CardData::colors`]). Like [`Effect::AddMana`] it has an implicit
    /// subject (the controller) and needs no target, and an activated ability whose
    /// every effect is one of the two mana verbs is a mana ability
    /// ([`is_mana_ability`]).
    AddColorlessMana {
        /// How much colorless mana is produced.
        amount: u8,
    },
    /// The controller draws `count` cards. The subject is implicit (the
    /// controller), so this effect needs no target.
    DrawCard {
        /// How many cards the controller draws.
        count: u8,
    },
    /// Tap the single permanent this effect targets (e.g. `Tap target
    /// creature.`).
    ///
    /// Unlike [`Effect::AddMana`]/[`Effect::DrawCard`], whose subject is the
    /// controller, this effect names an explicit subject. The `target` field is
    /// the [`TargetSpec`] constraining what may be chosen; the *chosen* value is
    /// a [`Target`] recorded on the [`crate::StackObject`] when the ability is
    /// put on the stack (CR 601.2c) and re-checked against current state on
    /// resolution (CR 608.2b — see the resolve path).
    Tap {
        /// What this effect is allowed to target.
        target: TargetSpec,
    },
    /// Counter the single spell on the stack this effect targets (CR 701.5a):
    /// on resolution the targeted spell is removed from the stack without
    /// resolving and put into its owner's graveyard. The first counterspell.
    ///
    /// Like [`Effect::Tap`], the subject is an explicit target rather than the
    /// controller: `target` is the [`TargetSpec`] (a [`TargetSpec::SpellOnStack`])
    /// constraining what may be chosen, the *chosen* value is a [`Target::Spell`]
    /// recorded on the [`crate::StackObject`] at cast (CR 601.2c) and re-checked on
    /// resolution (CR 608.2b) — a spell whose target already resolved fizzles.
    CounterSpell {
        /// What this effect is allowed to target (a spell on the stack).
        target: TargetSpec,
    },
    /// Deal `amount` damage to the single target this effect names (CR 120.3).
    ///
    /// The subject is an explicit target (like [`Effect::Tap`]), chosen at cast
    /// (CR 601.2c) and re-checked on resolution (CR 608.2b). Damage to a creature
    /// is *marked* on it (CR 120.3d) for the lethal-damage state-based action to
    /// read (CR 704.5g); damage to a player is *lost life* (CR 120.3a), feeding
    /// the zero-life state-based action (CR 704.5a). Damage prevention/replacement
    /// and deathtouch are not modeled.
    DealDamage {
        /// What this effect is allowed to target (a creature, a player, or — for
        /// a burn spell — [`TargetSpec::AnyTarget`]).
        target: TargetSpec,
        /// How much damage is dealt.
        amount: u32,
    },
    /// Destroy the single permanent this effect targets (CR 701.7): it is put
    /// into its owner's graveyard, the same graveyard path as lethal damage
    /// (CR 704.5g). Regeneration and other destruction-replacement effects are
    /// out of scope.
    ///
    /// Like [`Effect::Tap`] the subject is an explicit target, chosen at cast
    /// (CR 601.2c) and re-checked on resolution (CR 608.2b) — a destroy whose
    /// target has already left fizzles.
    Destroy {
        /// What this effect is allowed to target (typically a creature).
        target: TargetSpec,
    },
    /// Exile the single permanent this effect targets (CR 406.2 / CR 701.19): it is
    /// moved from the battlefield to its owner's exile zone through the one
    /// battlefield→exile seam ([`crate::GameState::move_permanent_to_exile`]), the
    /// exile counterpart of [`Effect::Destroy`]'s graveyard path. A commander so
    /// exiled offers its owner the CR 903.9a return-to-command-zone choice.
    ///
    /// Like [`Effect::Tap`] the subject is an explicit target, chosen at cast
    /// (CR 601.2c) and re-checked on resolution (CR 608.2b) — an exile whose target
    /// has already left fizzles. Exile-matters riders (impulse draw, flicker, "until
    /// this leaves") are out of scope: the object simply goes to exile and stays.
    Exile {
        /// What this effect is allowed to target (typically a creature or permanent).
        target: TargetSpec,
    },
    /// The referenced player gains `amount` life (CR 119.3). The subject is a
    /// non-targeted [`PlayerRef`] (like [`Effect::DrawCard`]'s implicit
    /// controller), so this effect chooses no target.
    GainLife {
        /// Which player gains the life.
        player_ref: PlayerRef,
        /// How much life is gained.
        amount: u32,
    },
    /// The referenced player loses `amount` life (CR 119.3). The subject is a
    /// non-targeted [`PlayerRef`]; life loss can drive the zero-life state-based
    /// action (CR 704.5a). This effect chooses no target.
    LoseLife {
        /// Which player loses the life.
        player_ref: PlayerRef,
        /// How much life is lost.
        amount: u32,
    },
    /// Put `count` counters of `kind` on the single permanent this effect targets
    /// (CR 122). Both `+1/+1` and `-1/-1` kinds are supported; they fold into the
    /// permanent's computed power/toughness (CR 613.7c) on demand, so a `-1/-1`
    /// counter can lower toughness to at or below marked damage and let the
    /// lethal-damage state-based action destroy it (CR 704.5g).
    ///
    /// Like [`Effect::Tap`] the subject is an explicit target, chosen at cast
    /// (CR 601.2c) and re-checked on resolution (CR 608.2b).
    PutCounters {
        /// What this effect is allowed to target (a permanent that can bear
        /// counters).
        target: TargetSpec,
        /// The kind of counter to place. Named `counter` on the wire because the
        /// effect enum already reserves the `kind` tag for its own discriminant.
        counter: CounterKind,
        /// How many counters of that kind to place.
        count: u32,
    },
    /// Give the single creature this effect targets `+power`/`+toughness`
    /// **until end of turn** — the pump-spell verb (e.g. `Target creature gets
    /// +3/+3 until end of turn.`). On resolution it adds a timestamped CR 613
    /// layer-7c power/toughness modifier that the cleanup step removes (CR 514.2).
    ///
    /// Like [`Effect::Tap`] the subject is an explicit target, chosen at cast
    /// (CR 601.2c) and re-checked on resolution (CR 608.2b). The amounts are
    /// signed, so a negative value is a shrink; the modifier folds into computed
    /// power/toughness on demand (CR 613.7c), after counters and in timestamp
    /// order, so two pumps in a turn stack and both wear off at cleanup.
    Pump {
        /// What this effect is allowed to target (a creature).
        target: TargetSpec,
        /// The signed amount added to the target's power until end of turn.
        power: i32,
        /// The signed amount added to the target's toughness until end of turn.
        toughness: i32,
    },
    /// Grant the single creature this effect targets a keyword ability **until end
    /// of turn** — the pump-spell analogue of [`Effect::Pump`] for keywords (e.g.
    /// `Target creature gains trample until end of turn.`, CR 702). On resolution it
    /// adds a CR 613 **layer-6** [`Modification::GrantKeyword`](crate::Modification::GrantKeyword)
    /// keyed to that one permanent, with an `UntilEndOfTurn` duration the cleanup
    /// step removes (CR 514.2).
    ///
    /// Like [`Effect::Tap`] the subject is an explicit target, chosen at cast
    /// (CR 601.2c) and re-checked on resolution (CR 608.2b). The granted keyword
    /// folds into the target's computed keyword set on demand (CR 613.1f) and is
    /// indistinguishable from a printed keyword; a duplicate grant is redundant, not
    /// additive.
    GrantKeyword {
        /// What this effect is allowed to target (a creature).
        target: TargetSpec,
        /// The keyword ability granted until end of turn.
        keyword: Keyword,
    },
    /// Return the single permanent this effect targets to its owner's **hand**
    /// (CR 400.7 — the bounce verb, e.g. `Return target creature to its owner's
    /// hand.`). It leaves the battlefield through the one battlefield→hand seam
    /// ([`crate::GameState::return_permanent_to_hand`]), the hand counterpart of
    /// [`Effect::Destroy`]'s graveyard path and [`Effect::Exile`]'s exile path.
    ///
    /// Like [`Effect::Tap`] the subject is an explicit target, chosen at cast
    /// (CR 601.2c) and re-checked on resolution (CR 608.2b). The permanent's
    /// [`crate::PermanentId`] is dropped and a later recast is a brand-new object, so
    /// this is *not* a death and fires no dies trigger (CR 603.6c).
    ReturnToHand {
        /// What this effect is allowed to target (typically a creature).
        target: TargetSpec,
    },
    /// Give **every permanent in a named class** `+power`/`+toughness` until end of
    /// turn — the mass counterpart of [`Effect::Pump`] (e.g. `Creatures you control
    /// get +2/+1 until end of turn.`). Chooses no target: a class is not a target
    /// (CR 115.1), so this never fizzles.
    ///
    /// The affected set is **locked in on resolution** (CR 611.2c): the class is
    /// enumerated once and one modifier is keyed to each permanent found, so a
    /// creature that arrives later in the turn is untouched — which is the whole
    /// difference between a one-shot pump and an anthem.
    PumpAll {
        /// The class of permanents modified.
        affects: MassAffects,
        /// The signed amount added to each affected permanent's power.
        power: i32,
        /// The signed amount added to each affected permanent's toughness.
        toughness: i32,
    },
    /// Grant **every permanent in a named class** a keyword ability until end of turn
    /// — the mass counterpart of [`Effect::GrantKeyword`] (e.g. `Creatures you
    /// control gain trample until end of turn.`). Chooses no target, and locks its
    /// affected set in on resolution exactly as [`Effect::PumpAll`] does.
    GrantKeywordAll {
        /// The class of permanents granted the keyword.
        affects: MassAffects,
        /// The keyword ability granted until end of turn.
        keyword: Keyword,
    },
    /// Impose a [`CombatRestriction`] on the single creature this effect targets
    /// **until end of turn** — the restriction counterpart of [`Effect::GrantKeyword`]
    /// (e.g. `Target creature can't be blocked this turn.`). On resolution it adds a
    /// CR 613 **layer-6** [`Modification::GrantRestriction`](crate::Modification::GrantRestriction)
    /// keyed to that one permanent, with an `UntilEndOfTurn` duration the cleanup step
    /// removes (CR 514.2).
    ///
    /// Like [`Effect::Tap`] the subject is an explicit target, chosen at cast
    /// (CR 601.2c) and re-checked on resolution (CR 608.2b). The restriction folds into
    /// the target's computed restrictions on demand and binds exactly as a printed one
    /// does; a duplicate imposition is redundant, not additive.
    Restrict {
        /// What this effect is allowed to target (a creature).
        target: TargetSpec,
        /// The restriction imposed until end of turn.
        restriction: CombatRestriction,
    },
    /// Impose a [`CombatRestriction`] on **this ability's own source** until end of turn
    /// — the self-referential counterpart of [`Effect::Restrict`] (`this creature can't
    /// be blocked this turn`), and the restriction counterpart of [`Effect::PumpSelf`].
    ///
    /// The subject is implicit: the source is not a *target* (CR 115.1), so this chooses
    /// nothing, fills no slot, and can never fizzle. A source that has left the
    /// battlefield is not there to restrict, and the effect does nothing.
    RestrictSelf {
        /// The restriction imposed on the source until end of turn.
        restriction: CombatRestriction,
    },
    /// Impose a [`CombatRestriction`] on **every permanent in a named class** until end
    /// of turn — the mass counterpart of [`Effect::Restrict`] (e.g. `Creatures without
    /// flying can't block this turn.`). Chooses no target, and locks its affected set in
    /// on resolution exactly as [`Effect::PumpAll`] does (CR 611.2c).
    RestrictAll {
        /// The class of permanents restricted.
        affects: MassAffects,
        /// The restriction imposed until end of turn.
        restriction: CombatRestriction,
    },
    /// Give **this ability's own source** `+power`/`+toughness` until end of turn —
    /// the self-referential counterpart of [`Effect::Pump`] (`this creature gets +1/+1
    /// until end of turn`).
    ///
    /// The subject is implicit, like [`Effect::DrawCard`]'s controller: the source is
    /// not a *target* (CR 115.1), so this chooses nothing, fills no slot, and can
    /// never fizzle. A source that has left the battlefield by the time the ability
    /// resolves is simply not there to modify, and the effect does nothing.
    PumpSelf {
        /// The signed amount added to the source's power until end of turn.
        power: i32,
        /// The signed amount added to the source's toughness until end of turn.
        toughness: i32,
    },
    /// Put `count` counters of `counter` on **this ability's own source** (CR 122) —
    /// the self-referential counterpart of [`Effect::PutCounters`] (`put a +1/+1
    /// counter on this creature`). Like [`Effect::PumpSelf`] the subject is implicit
    /// and no target is chosen.
    PutCountersOnSelf {
        /// The kind of counter to place. Named `counter` on the wire because the
        /// effect enum already reserves the `kind` tag for its own discriminant.
        counter: CounterKind,
        /// How many counters of that kind to place.
        count: u32,
    },
    /// The referenced player puts the top `count` cards of their library into their
    /// graveyard (CR 701.13, "mill"). Milling an empty library simply moves fewer
    /// cards — it is not a draw, so it never triggers the CR 704.5c decking loss.
    ///
    /// The subject is a [`PlayerRef`], which decides on its own whether a target is
    /// chosen ([`PlayerRef::target_spec`]): `each_opponent` mills every opponent and
    /// fizzles never, while `target_player` occupies a target slot.
    Mill {
        /// Which player mills.
        player_ref: PlayerRef,
        /// How many cards are put into that player's graveyard.
        count: u8,
    },
}

/// **Which player** a player-subject effect (e.g. [`Effect::GainLife`]) acts on.
///
/// A closed, plain-data enum deserialized from a bare `snake_case` tag, e.g.
/// `{"kind": "gain_life", "player_ref": "controller", "amount": 3}`.
///
/// The reference itself declares whether a *target* is chosen (CR 115.1). That is
/// what [`Self::target_spec`] answers, and it is the whole difference between "each
/// opponent loses 2 life" — which chooses nothing and can never fizzle — and "target
/// opponent loses 2 life", which occupies a target slot at announcement (CR 601.2c)
/// and is re-checked on resolution (CR 608.2b). Keeping that fact on the *reference*
/// rather than on each effect means a new life-, mill-, or draw-style effect gets both
/// shapes for free and cannot get the fizzle rule wrong for one of them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlayerRef {
    /// The controller of the spell or ability producing the effect ("you").
    /// Chooses no target.
    Controller,
    /// Every opponent of the controller still in the game — the "each opponent" of a
    /// symmetric drain (CR 102.1). Chooses no target, so it never fizzles and, in a
    /// game of three or more, really does hit every opponent rather than one.
    EachOpponent,
    /// One **targeted** player (CR 115.1), any seat still in the game: "target player".
    TargetPlayer,
    /// One **targeted** opponent of the controller: "target opponent". Distinct from
    /// [`Self::TargetPlayer`] because the legal set excludes the controller
    /// themselves, which for a life-loss effect is the difference between a drain and
    /// a way to lose the game.
    TargetOpponent,
}

impl PlayerRef {
    /// The [`TargetSpec`] this reference chooses a target for, or `None` when it
    /// names its player without targeting (CR 115.1).
    ///
    /// Exhaustive, so a new variant must declare which side of the targeting line it
    /// falls on; [`Effect::target_spec`] defers to this for every player-subject
    /// effect, and the resolve path pairs the stored [`Target`] with the effect from
    /// the same answer.
    #[must_use]
    pub fn target_spec(self) -> Option<TargetSpec> {
        match self {
            PlayerRef::Controller | PlayerRef::EachOpponent => None,
            PlayerRef::TargetPlayer => Some(TargetSpec::AnyPlayer),
            PlayerRef::TargetOpponent => Some(TargetSpec::AnyOpponent),
        }
    }
}

/// The class of permanents a **mass, non-targeting** effect ([`Effect::PumpAll`],
/// [`Effect::GrantKeywordAll`]) applies to.
///
/// Deliberately separate from [`StaticAffects`], which selects for a *continuous*
/// ability and carries an `except_this` that a one-shot spell has no "this" for. A
/// closed enum deserialized from a bare `snake_case` tag, e.g.
/// `{"kind":"pump_all","affects":"creatures_you_control","power":2,"toughness":1}`.
/// It grows by adding variants (attacking creatures, creatures your opponents
/// control, …) as cards need them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MassAffects {
    /// Every creature the effect's controller controls at the moment it resolves.
    CreaturesYouControl,
    /// Every creature controlled by an opponent of the effect's controller who is still
    /// in the game, at the moment it resolves — the symmetric sweeper's scope. In a game
    /// of three or more this really is every opponent's board, which is why it is not
    /// spelled "the opponent's creatures".
    CreaturesYourOpponentsControl,
    /// Every creature on the battlefield that does not currently have flying, whoever
    /// controls it — the scope of an effect that clears the ground.
    ///
    /// Flying is read through the computed keywords (CR 613.1f), so a creature that was
    /// *granted* flying is outside the class exactly as a printed flyer is. The class is
    /// still evaluated once, on resolution (CR 611.2c), like every other mass effect: a
    /// creature that loses flying later in the turn does not retroactively join it.
    CreaturesWithoutFlying,
}

impl Effect {
    /// The [`TargetSpec`] this effect must be given a chosen target for, or
    /// `None` for an effect with an implicit subject ([`Effect::AddMana`],
    /// [`Effect::DrawCard`]).
    ///
    /// The resolution path uses this to pair each of an object's stored
    /// [`Target`]s with the effect that consumes it and to re-check that
    /// target's legality (CR 608.2b). Kept exhaustive so a new targeting
    /// [`Effect`] variant must declare its spec here.
    #[must_use]
    pub fn target_spec(&self) -> Option<TargetSpec> {
        match self {
            Effect::Tap { target }
            | Effect::CounterSpell { target }
            | Effect::DealDamage { target, .. }
            | Effect::Destroy { target }
            | Effect::Exile { target }
            | Effect::PutCounters { target, .. }
            | Effect::Pump { target, .. }
            | Effect::GrantKeyword { target, .. }
            | Effect::Restrict { target, .. }
            | Effect::ReturnToHand { target } => Some(*target),
            // A player-subject effect targets exactly when its reference does
            // (CR 115.1) — "target opponent loses 2 life" fills a slot, "each
            // opponent loses 2 life" fills none. One answer, from the reference.
            Effect::GainLife { player_ref, .. }
            | Effect::LoseLife { player_ref, .. }
            | Effect::Mill { player_ref, .. } => player_ref.target_spec(),
            Effect::AddMana { .. }
            | Effect::AddColorlessMana { .. }
            | Effect::DrawCard { .. }
            // A class of permanents is not a target (CR 115.1), and neither is the
            // ability's own source.
            | Effect::PumpAll { .. }
            | Effect::GrantKeywordAll { .. }
            | Effect::RestrictAll { .. }
            | Effect::PumpSelf { .. }
            | Effect::RestrictSelf { .. }
            | Effect::PutCountersOnSelf { .. } => None,
        }
    }
}

/// A **target spec**: what an [`Effect`] is allowed to target, authored as card
/// data alongside the rest of the IR (CR 115.1 "target … as defined by the
/// spell or ability").
///
/// This is a declaration, not a chosen value: it names a *class* of legal
/// objects, while a [`Target`] names one specific object the player picked. The
/// engine turns a spec into the concrete legal set on demand (enumeration is
/// issue #71) and re-checks a chosen [`Target`] against it on resolution.
///
/// A closed, plain-data enum (no closures — ADR 0003) deserialized from a bare
/// string tag, e.g. `{"kind": "tap", "target": "any_creature"}`. It grows by
/// adding variants (any permanent of a type, an object in a named zone, …).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Every spec is evaluated **relative to the object's controller**, which is why
/// the legality predicate takes one: `any_creature_you_control` and
/// `any_creature_an_opponent_controls` name different sets for different players,
/// and the same authored card must mean "you" from either seat.
pub enum TargetSpec {
    /// Any player in the game.
    AnyPlayer,
    /// Any **opponent** of the object's controller still in the game — "target
    /// opponent". The controller themselves is never a candidate.
    AnyOpponent,
    /// Any permanent on the battlefield.
    AnyPermanent,
    /// Any permanent that is not a land — "target nonland permanent".
    AnyNonlandPermanent,
    /// Any creature on the battlefield (a permanent whose printed types include
    /// [`crate::CardType::Creature`]).
    AnyCreature,
    /// Any creature the object's controller controls — "target creature you control".
    AnyCreatureYouControl,
    /// Any creature controlled by an opponent of the object's controller — "target
    /// creature an opponent controls".
    AnyCreatureAnOpponentControls,
    /// Any creature that currently has flying (CR 613.1f, so a granted flying counts
    /// exactly as a printed one) — the "target creature with flying" of an anti-air
    /// removal spell.
    AnyCreatureWithFlying,
    /// Any creature that is currently tapped — "target tapped creature".
    AnyTappedCreature,
    /// Any artifact on the battlefield.
    AnyArtifact,
    /// Any enchantment on the battlefield.
    AnyEnchantment,
    /// Any artifact **or** enchantment — the single slot of a naturalize-style
    /// spell, which is one target of either type rather than two slots.
    AnyArtifactOrEnchantment,
    /// Any land on the battlefield.
    AnyLand,
    /// Any spell on the stack — a [`crate::StackObjectKind::Spell`] object (CR
    /// 701.5, "counter target spell"). Abilities on the stack are not spells and
    /// are never candidates; a mana ability never uses the stack at all (CR
    /// 605.3), so it can never be countered.
    SpellOnStack,
    /// Any **creature** spell on the stack — the narrower counterspell of
    /// `Essence Scatter`. A creature spell is a spell whose card's printed types
    /// include creature; the check is on the card on the stack, not on any
    /// permanent, because it has not entered the battlefield yet.
    CreatureSpellOnStack,
    /// Any target (CR 115.4): the modern "any target" of a burn spell — any
    /// creature on the battlefield or any player still in the game. Planeswalkers
    /// and battles are not modeled, so the legal set is exactly creatures plus
    /// players.
    AnyTarget,
}

/// A **chosen target**: a resolved reference to one specific game object the
/// player aimed a spell or ability at (CR 601.2c).
///
/// Names a specific instance/permanent/player by its per-game identity, never a
/// bare printed [`crate::CardId`] — two copies of one printing must stay
/// distinguishable (per-instance identity, issue #51). Stored on the
/// [`crate::StackObject`] and re-checked against its [`TargetSpec`] on
/// resolution; this value type is the one issue #71 will also carry on a
/// parameterized `Action`.
///
/// Plain `Copy`/`Eq` data with no closures, so [`crate::GameState`] keeps its
/// value semantics.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Target {
    /// A specific player, by seat.
    Player(PlayerId),
    /// A specific permanent on the battlefield, by its battlefield identity.
    Permanent(PermanentId),
    /// A specific physical card, by its per-game instance identity (for targets
    /// that name a card in a zone rather than a permanent).
    Card(CardInstanceId),
    /// A specific object on the stack, by its [`StackId`] (for targets that name
    /// a spell on the stack, e.g. a counterspell — CR 701.5).
    Spell(StackId),
}

/// The condition under which a [`Ability::Triggered`] triggers.
///
/// Each variant is evaluated by [`fire_count`](crate::triggers) as a pure function of
/// the states before and after an action — never via an event listener.
///
/// Authored in serde's **externally tagged** form rather than the internal `kind` tag
/// the effect vocabulary uses, because the three original conditions are authored as
/// bare strings (`"event": "self_dies"`) and changing that would be a breaking schema
/// migration for every existing card to buy nothing. A condition that carries a
/// selector wraps it instead:
/// `"event": {"permanent_dies": {"scope": "any_creature", "except_this": true}}`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriggerCondition {
    /// The source permanent entered the battlefield this transition (its
    /// [`crate::PermanentId`] is present after but not before).
    SelfEntersBattlefield,
    /// The source permanent **died** this transition: it left the battlefield for
    /// a graveyard (CR 700.4, the "dies" event of CR 603.6c). Observed by diff —
    /// its [`crate::PermanentId`] is present before but not after, and its physical
    /// instance is now in a graveyard it was not in before. A leave to any
    /// non-graveyard zone does not satisfy this, so a future bounce or exile never
    /// fires it. Fires from any cause (lethal damage, `Destroy`, or combat), all
    /// through the one leaves-battlefield seam
    /// ([`crate::GameState::move_permanent_to_graveyard`]).
    SelfDies,
    /// A permanent matching `observes` **entered the battlefield** this transition
    /// (CR 603.6b) — the first condition here that watches something other than its
    /// own source, e.g. `Whenever a creature you control enters, …`.
    ///
    /// Observed by the same before/after diff the self conditions use: the permanents
    /// present after and absent before. It fires **once per matching permanent**, so
    /// two creatures entering at once trigger it twice.
    ///
    /// The source must still be on the battlefield after the transition — an ability
    /// that left cannot watch the board it is no longer on.
    PermanentEnters(
        /// Which permanents entering satisfy this condition.
        ObservedPermanent,
    ),
    /// A permanent matching `observes` **died** this transition (CR 700.4), e.g.
    /// `Whenever another creature dies, …`. The counterpart of [`Self::SelfDies`] for
    /// an ability watching the rest of the board, and observed the same way: it left
    /// the battlefield for a graveyard.
    ///
    /// Fires **once per matching death**, so a board wipe triggers it many times.
    /// Unlike [`Self::PermanentEnters`], the source need *not* have survived: two
    /// creatures dying simultaneously is one death this ability observes and one it
    /// is, and `except_this` is what keeps the two apart.
    PermanentDies(
        /// Which permanents dying satisfy this condition.
        ObservedPermanent,
    ),
    /// The source's controller **gained life** this transition (CR 118.3), e.g.
    /// `Whenever you gain life, …`.
    ///
    /// Read from the events this transition recorded rather than by comparing life
    /// totals, because the trigger is about the *event*, not the net: gaining three
    /// and losing three is a life gain that triggers, and a comparison of totals
    /// would see nothing happen. Life lost is not a gain, and damage is never one
    /// (damage to a player is recorded as damage, not as a life change), so neither
    /// fires this. Fires once per life-gain event.
    YouGainLife,
    /// The source's controller **cast a spell** matching `spell` this transition
    /// (CR 601), e.g. `Whenever you cast an enchantment spell, …`.
    ///
    /// Read from the recorded cast events, so it fires as the spell goes on the
    /// stack — before it resolves, and whether or not it ever does. Fires once per
    /// matching cast.
    YouCastSpell(
        /// Which spells satisfy this condition.
        ObservedSpell,
    ),
    /// The source permanent was **declared as an attacker** this transition (CR
    /// 508.1, the "attacks" event of CR 603.6d). Observed by diff like every other
    /// condition here — its [`crate::state::Permanent::attacking`] is set after and
    /// was not before — so it fires once per declaration, from the one place
    /// attackers are declared, and never from a creature that merely became tapped.
    SelfAttacks,
}

/// Which permanents a **watching** [`TriggerCondition`] observes.
///
/// The observer's counterpart to [`StaticAffects`], and deliberately the same shape: a
/// closed selector naming a *class*, evaluated against each candidate relative to the
/// watching ability's own source. Kept separate from `StaticAffects` because the two
/// answer different questions — one selects permanents to *modify* continuously, this
/// one selects events to *notice* — and collapsing them would make a future variant
/// meaningful for one and nonsense for the other.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "scope", rename_all = "snake_case")]
pub enum ObservedPermanent {
    /// Creatures controlled by the watching ability's controller — "a creature you
    /// control".
    CreaturesYouControl {
        /// Restrict to creatures whose subtypes include this one ("whenever a
        /// **Dragon** you control enters"). Absent means every creature.
        #[serde(default)]
        subtype: Option<String>,
        /// Exclude the watching ability's own source — the "another" of "whenever
        /// another creature you control enters". Compares the *permanent*, so two
        /// copies of one card do notice each other.
        #[serde(default)]
        except_this: bool,
    },
    /// Any creature on the battlefield, whoever controls it — "a creature", and with
    /// `except_this`, "another creature".
    AnyCreature {
        /// Restrict to creatures whose subtypes include this one.
        #[serde(default)]
        subtype: Option<String>,
        /// Exclude the watching ability's own source.
        #[serde(default)]
        except_this: bool,
    },
}

impl ObservedPermanent {
    /// Whether `except_this` is set — whether the source excludes itself.
    #[must_use]
    pub fn excludes_source(&self) -> bool {
        match self {
            ObservedPermanent::CreaturesYouControl { except_this, .. }
            | ObservedPermanent::AnyCreature { except_this, .. } => *except_this,
        }
    }

    /// The subtype this selector restricts to, if any.
    #[must_use]
    pub fn subtype(&self) -> Option<&str> {
        match self {
            ObservedPermanent::CreaturesYouControl { subtype, .. }
            | ObservedPermanent::AnyCreature { subtype, .. } => subtype.as_deref(),
        }
    }
}

/// Which spells a [`TriggerCondition::YouCastSpell`] notices.
///
/// A closed set deserialized from a bare `snake_case` tag. Deliberately named by the
/// classes real cards ask about rather than by card type, because "instant or sorcery"
/// is one class to a card and two types to the engine.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservedSpell {
    /// An enchantment spell.
    Enchantment,
    /// An instant **or** sorcery spell — one class, as a card writes it.
    InstantOrSorcery,
}

/// Whether an ability is a mana ability (CR 605.1a, simplified): an activated
/// ability whose every effect adds mana. Mana abilities resolve immediately and
/// do not use the stack (see `crate::apply_action`). Derived, never stored.
#[must_use]
pub fn is_mana_ability(ability: &Ability) -> bool {
    matches!(
        ability,
        Ability::Activated { effects, .. }
            if !effects.is_empty()
                && effects.iter().all(|e| matches!(
                    e,
                    Effect::AddMana { .. } | Effect::AddColorlessMana { .. }
                ))
    )
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn activated_mana_ability_round_trips() {
        let json = r#"{"type":"activated","cost":[{"kind":"tap"}],"effects":[{"kind":"add_mana","color":"green","amount":1}]}"#;
        let ability: Ability = serde_json::from_str(json).unwrap();
        assert_eq!(
            ability,
            Ability::Activated {
                cost: vec![Cost::Tap],
                effects: vec![Effect::AddMana {
                    color: Color::Green,
                    amount: 1
                }],
            }
        );
        assert!(is_mana_ability(&ability));
    }

    #[test]
    fn issue_256_activated_colorless_mana_ability_round_trips() {
        // A mana rock's {T}: Add {C} — an activated ability whose only effect is
        // colorless mana production. It round-trips and is recognized as a mana ability.
        let json = r#"{"type":"activated","cost":[{"kind":"tap"}],"effects":[{"kind":"add_colorless_mana","amount":1}]}"#;
        let ability: Ability = serde_json::from_str(json).unwrap();
        assert_eq!(
            ability,
            Ability::Activated {
                cost: vec![Cost::Tap],
                effects: vec![Effect::AddColorlessMana { amount: 1 }],
            }
        );
        assert!(is_mana_ability(&ability));
        // Colorless mana production has an implicit subject, so it targets nothing.
        assert_eq!(Effect::AddColorlessMana { amount: 1 }.target_spec(), None);
    }

    #[test]
    fn triggered_etb_draw_round_trips() {
        let json = r#"{"type":"triggered","event":"self_enters_battlefield","effects":[{"kind":"draw_card","count":1}]}"#;
        let ability: Ability = serde_json::from_str(json).unwrap();
        assert_eq!(
            ability,
            Ability::Triggered {
                event: TriggerCondition::SelfEntersBattlefield,
                effects: vec![Effect::DrawCard { count: 1 }],
            }
        );
        assert!(!is_mana_ability(&ability));
    }

    #[test]
    fn issue_151_triggered_dies_draw_round_trips() {
        // The dies trigger authors its condition as the bare `self_dies` tag
        // (CR 700.4 / 603.6c) and reuses the draw effect.
        let json = r#"{"type":"triggered","event":"self_dies","effects":[{"kind":"draw_card","count":1}]}"#;
        let ability: Ability = serde_json::from_str(json).unwrap();
        assert_eq!(
            ability,
            Ability::Triggered {
                event: TriggerCondition::SelfDies,
                effects: vec![Effect::DrawCard { count: 1 }],
            }
        );
        assert!(!is_mana_ability(&ability));
    }

    #[test]
    fn issue_155_enters_tapped_replacement_round_trips() {
        // The "enters tapped" self-replacement (CR 614.1c) authors as the bare
        // `enters_tapped` type tag and is not a mana ability.
        let json = r#"{"type":"enters_tapped"}"#;
        let ability: Ability = serde_json::from_str(json).unwrap();
        assert_eq!(ability, Ability::EntersTapped);
        assert!(!is_mana_ability(&ability));
    }

    #[test]
    fn issue_155_enters_with_counters_replacement_round_trips() {
        // The "enters with N counters" self-replacement (CR 614.12) authors its
        // counter kind under `counter` (the enum reserves `type` for its tag) and
        // its count as data.
        let json = r#"{"type":"enters_with_counters","counter":"plus_one_plus_one","count":2}"#;
        let ability: Ability = serde_json::from_str(json).unwrap();
        assert_eq!(
            ability,
            Ability::EntersWithCounters {
                counter: CounterKind::PlusOnePlusOne,
                count: 2,
            }
        );
        assert!(!is_mana_ability(&ability));
    }

    #[test]
    fn activated_non_mana_ability_is_not_a_mana_ability() {
        let ability = Ability::Activated {
            cost: vec![Cost::Tap],
            effects: vec![Effect::DrawCard { count: 1 }],
        };
        assert!(!is_mana_ability(&ability));
    }

    #[test]
    fn tap_effect_round_trips_with_its_target_spec() {
        // The target spec is authored as a bare string tag on the effect.
        let json = r#"{"kind":"tap","target":"any_creature"}"#;
        let effect: Effect = serde_json::from_str(json).unwrap();
        assert_eq!(
            effect,
            Effect::Tap {
                target: TargetSpec::AnyCreature,
            }
        );
    }

    #[test]
    fn target_spec_variants_deserialize_from_bare_strings() {
        assert_eq!(
            serde_json::from_str::<TargetSpec>(r#""any_player""#).unwrap(),
            TargetSpec::AnyPlayer
        );
        assert_eq!(
            serde_json::from_str::<TargetSpec>(r#""any_permanent""#).unwrap(),
            TargetSpec::AnyPermanent
        );
    }

    #[test]
    fn only_targeting_effects_report_a_target_spec() {
        // A targeting effect exposes its spec; implicit-subject effects do not.
        assert_eq!(
            Effect::Tap {
                target: TargetSpec::AnyPermanent,
            }
            .target_spec(),
            Some(TargetSpec::AnyPermanent)
        );
        assert_eq!(Effect::DrawCard { count: 1 }.target_spec(), None);
        assert_eq!(
            Effect::AddMana {
                color: Color::Green,
                amount: 1
            }
            .target_spec(),
            None
        );
    }

    #[test]
    fn counter_spell_effect_round_trips_with_its_target_spec() {
        // The counterspell effect authors its spec as a bare string tag, and only
        // it (a targeting effect) reports a spec (CR 701.5).
        let json = r#"{"kind":"counter_spell","target":"spell_on_stack"}"#;
        let effect: Effect = serde_json::from_str(json).unwrap();
        assert_eq!(
            effect,
            Effect::CounterSpell {
                target: TargetSpec::SpellOnStack,
            }
        );
        assert_eq!(effect.target_spec(), Some(TargetSpec::SpellOnStack));
        assert_eq!(
            serde_json::from_str::<TargetSpec>(r#""spell_on_stack""#).unwrap(),
            TargetSpec::SpellOnStack
        );
    }

    #[test]
    fn a_tap_effect_is_not_a_mana_ability() {
        let ability = Ability::Activated {
            cost: vec![Cost::Tap],
            effects: vec![Effect::Tap {
                target: TargetSpec::AnyCreature,
            }],
        };
        assert!(!is_mana_ability(&ability));
    }

    #[test]
    fn issue_149_deal_damage_round_trips_with_its_target_spec() {
        let json = r#"{"kind":"deal_damage","target":"any_target","amount":2}"#;
        let effect: Effect = serde_json::from_str(json).unwrap();
        assert_eq!(
            effect,
            Effect::DealDamage {
                target: TargetSpec::AnyTarget,
                amount: 2,
            }
        );
        // A targeting effect reports its spec; the "any target" spec deserializes
        // from its bare string tag.
        assert_eq!(effect.target_spec(), Some(TargetSpec::AnyTarget));
        assert_eq!(
            serde_json::from_str::<TargetSpec>(r#""any_target""#).unwrap(),
            TargetSpec::AnyTarget
        );
    }

    #[test]
    fn issue_149_destroy_round_trips_with_its_target_spec() {
        let json = r#"{"kind":"destroy","target":"any_creature"}"#;
        let effect: Effect = serde_json::from_str(json).unwrap();
        assert_eq!(
            effect,
            Effect::Destroy {
                target: TargetSpec::AnyCreature,
            }
        );
        assert_eq!(effect.target_spec(), Some(TargetSpec::AnyCreature));
    }

    #[test]
    fn issue_149_put_counters_round_trips_with_both_kinds() {
        // The counter kind is authored under `counter` (the enum reserves `kind`
        // for its own tag) and deserializes from a snake_case string.
        let plus = r#"{"kind":"put_counters","target":"any_creature","counter":"plus_one_plus_one","count":1}"#;
        assert_eq!(
            serde_json::from_str::<Effect>(plus).unwrap(),
            Effect::PutCounters {
                target: TargetSpec::AnyCreature,
                counter: CounterKind::PlusOnePlusOne,
                count: 1,
            }
        );
        let minus = r#"{"kind":"put_counters","target":"any_creature","counter":"minus_one_minus_one","count":2}"#;
        assert_eq!(
            serde_json::from_str::<Effect>(minus).unwrap(),
            Effect::PutCounters {
                target: TargetSpec::AnyCreature,
                counter: CounterKind::MinusOneMinusOne,
                count: 2,
            }
        );
    }

    #[test]
    fn issue_150_pump_round_trips_with_its_target_spec() {
        // The pump verb authors its target spec and signed P/T amounts as card
        // data, and (a targeting effect) reports its spec.
        let json = r#"{"kind":"pump","target":"any_creature","power":3,"toughness":3}"#;
        let effect: Effect = serde_json::from_str(json).unwrap();
        assert_eq!(
            effect,
            Effect::Pump {
                target: TargetSpec::AnyCreature,
                power: 3,
                toughness: 3,
            }
        );
        assert_eq!(effect.target_spec(), Some(TargetSpec::AnyCreature));
    }

    #[test]
    fn issue_374_grant_keyword_round_trips_with_its_target_spec() {
        // The keyword-granting pump verb authors its target spec and the keyword it
        // grants as card data, and (a targeting effect) reports its spec.
        use crate::card::Keyword;
        let json = r#"{"kind":"grant_keyword","target":"any_creature","keyword":"trample"}"#;
        let effect: Effect = serde_json::from_str(json).unwrap();
        assert_eq!(
            effect,
            Effect::GrantKeyword {
                target: TargetSpec::AnyCreature,
                keyword: Keyword::Trample,
            }
        );
        assert_eq!(effect.target_spec(), Some(TargetSpec::AnyCreature));
    }

    #[test]
    fn a_mana_activation_cost_round_trips_as_the_string_it_was_written_in() {
        // The authored card keeps its `{...}` notation; parsing happens on demand, so
        // the JSON a card is written in never has to mirror an internal cost shape.
        let json = r#"{"type":"activated","cost":[{"kind":"mana","mana":"{1}{R}"},{"kind":"tap"}],"effects":[{"kind":"draw_card","count":1}]}"#;
        let ability: Ability = serde_json::from_str(json).unwrap();
        assert_eq!(
            ability,
            Ability::Activated {
                cost: vec![
                    Cost::Mana {
                        mana: "{1}{R}".to_string()
                    },
                    Cost::Tap
                ],
                effects: vec![Effect::DrawCard { count: 1 }],
            }
        );
        // CR 605.1a is about the *effects*, not the cost: a mana cost does not stop an
        // ability being a mana ability, and a non-mana effect still does.
        assert!(!is_mana_ability(&ability));
        let rock: Ability = serde_json::from_str(
            r#"{"type":"activated","cost":[{"kind":"mana","mana":"{1}"},{"kind":"tap"}],"effects":[{"kind":"add_colorless_mana","amount":1}]}"#,
        )
        .unwrap();
        assert!(is_mana_ability(&rock));
    }

    #[test]
    fn a_player_reference_decides_for_itself_whether_it_targets() {
        // The fizzle rule follows from the reference, so every player-subject effect
        // inherits one consistent answer instead of restating it.
        assert_eq!(PlayerRef::Controller.target_spec(), None);
        assert_eq!(PlayerRef::EachOpponent.target_spec(), None);
        assert_eq!(
            PlayerRef::TargetPlayer.target_spec(),
            Some(TargetSpec::AnyPlayer)
        );
        assert_eq!(
            PlayerRef::TargetOpponent.target_spec(),
            Some(TargetSpec::AnyOpponent)
        );

        // …and the effects defer to it, rather than each hard-coding a slot.
        let drain = Effect::LoseLife {
            player_ref: PlayerRef::TargetOpponent,
            amount: 2,
        };
        assert_eq!(drain.target_spec(), Some(TargetSpec::AnyOpponent));
        let symmetric = Effect::LoseLife {
            player_ref: PlayerRef::EachOpponent,
            amount: 2,
        };
        assert_eq!(symmetric.target_spec(), None);
        let mill = Effect::Mill {
            player_ref: PlayerRef::TargetPlayer,
            count: 2,
        };
        assert_eq!(mill.target_spec(), Some(TargetSpec::AnyPlayer));
    }

    #[test]
    fn the_new_effect_verbs_round_trip_with_their_target_or_class() {
        let bounce = r#"{"kind":"return_to_hand","target":"any_creature"}"#;
        let bounce: Effect = serde_json::from_str(bounce).unwrap();
        assert_eq!(
            bounce,
            Effect::ReturnToHand {
                target: TargetSpec::AnyCreature
            }
        );
        assert_eq!(bounce.target_spec(), Some(TargetSpec::AnyCreature));

        let mill = r#"{"kind":"mill","player_ref":"each_opponent","count":2}"#;
        assert_eq!(
            serde_json::from_str::<Effect>(mill).unwrap(),
            Effect::Mill {
                player_ref: PlayerRef::EachOpponent,
                count: 2,
            }
        );

        // A mass modification names a class, which is not a target (CR 115.1).
        let pump =
            r#"{"kind":"pump_all","affects":"creatures_you_control","power":2,"toughness":1}"#;
        let pump: Effect = serde_json::from_str(pump).unwrap();
        assert_eq!(
            pump,
            Effect::PumpAll {
                affects: MassAffects::CreaturesYouControl,
                power: 2,
                toughness: 1,
            }
        );
        assert_eq!(pump.target_spec(), None);

        let grant =
            r#"{"kind":"grant_keyword_all","affects":"creatures_you_control","keyword":"trample"}"#;
        let grant: Effect = serde_json::from_str(grant).unwrap();
        assert_eq!(
            grant,
            Effect::GrantKeywordAll {
                affects: MassAffects::CreaturesYouControl,
                keyword: Keyword::Trample,
            }
        );
        assert_eq!(grant.target_spec(), None);
    }

    #[test]
    fn the_new_target_specs_deserialize_from_their_bare_string_tags() {
        for (tag, spec) in [
            ("any_opponent", TargetSpec::AnyOpponent),
            ("any_nonland_permanent", TargetSpec::AnyNonlandPermanent),
            (
                "any_creature_you_control",
                TargetSpec::AnyCreatureYouControl,
            ),
            (
                "any_creature_an_opponent_controls",
                TargetSpec::AnyCreatureAnOpponentControls,
            ),
            (
                "any_creature_with_flying",
                TargetSpec::AnyCreatureWithFlying,
            ),
            ("any_tapped_creature", TargetSpec::AnyTappedCreature),
            ("any_artifact", TargetSpec::AnyArtifact),
            ("any_enchantment", TargetSpec::AnyEnchantment),
            (
                "any_artifact_or_enchantment",
                TargetSpec::AnyArtifactOrEnchantment,
            ),
            ("any_land", TargetSpec::AnyLand),
            ("creature_spell_on_stack", TargetSpec::CreatureSpellOnStack),
        ] {
            let json = format!("\"{tag}\"");
            assert_eq!(
                serde_json::from_str::<TargetSpec>(&json).unwrap(),
                spec,
                "{tag}"
            );
        }
    }

    #[test]
    fn the_attacks_trigger_authors_its_condition_as_a_bare_tag() {
        let json = r#"{"type":"triggered","event":"self_attacks","effects":[{"kind":"gain_life","player_ref":"controller","amount":2}]}"#;
        let ability: Ability = serde_json::from_str(json).unwrap();
        assert_eq!(
            ability,
            Ability::Triggered {
                event: TriggerCondition::SelfAttacks,
                effects: vec![Effect::GainLife {
                    player_ref: PlayerRef::Controller,
                    amount: 2
                }],
            }
        );
    }

    #[test]
    fn issue_149_life_effects_round_trip_and_target_nothing() {
        let gain = r#"{"kind":"gain_life","player_ref":"controller","amount":3}"#;
        let gain: Effect = serde_json::from_str(gain).unwrap();
        assert_eq!(
            gain,
            Effect::GainLife {
                player_ref: PlayerRef::Controller,
                amount: 3,
            }
        );
        let lose = r#"{"kind":"lose_life","player_ref":"controller","amount":2}"#;
        let lose: Effect = serde_json::from_str(lose).unwrap();
        assert_eq!(
            lose,
            Effect::LoseLife {
                player_ref: PlayerRef::Controller,
                amount: 2,
            }
        );
        // Life gain/loss have an implicit subject, so they choose no target.
        assert_eq!(gain.target_spec(), None);
        assert_eq!(lose.target_spec(), None);
    }
}
