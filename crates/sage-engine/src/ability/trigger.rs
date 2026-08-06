//! Trigger conditions (CR 603): what a triggered ability watches for, matched by a pure
//! predicate against a before/after diff rather than by any listener.

use super::*;

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
    /// A permanent matching `observes` was **declared as an attacker** this transition
    /// (CR 508.1), e.g. `Whenever a creature with flying attacks, …` — the counterpart
    /// of [`Self::SelfAttacks`] for an ability watching the rest of the board, and the
    /// same shape [`Self::PermanentEnters`] and [`Self::PermanentDies`] take.
    ///
    /// Observed on the one field the declaration writes, exactly as the self form is:
    /// `attacking` is set after and was not before. It fires **once per matching
    /// attacker**, so a five-creature alpha strike triggers a watcher of them five
    /// times, and the selector is judged in the state the declaration produced — a
    /// creature granted flying before attackers were declared really is one this
    /// condition notices.
    ///
    /// The source must still be on the battlefield after the transition.
    PermanentAttacks(
        /// Which permanents attacking satisfy this condition.
        ObservedPermanent,
    ),
    /// The source's controller **drew a card** this transition (CR 121.1), e.g.
    /// `Whenever you draw a card, …`.
    ///
    /// Read from the [`crate::GameEvent::CardsDrawn`] entries this transition recorded
    /// rather than from hand size, and the difference is the whole condition: a draw
    /// followed by a discard leaves the hand exactly as it was and has still drawn a
    /// card, while a hand that grew by one may have grown from a search or a return.
    /// Only cards that actually moved are recorded, so a draw from an empty library —
    /// which draws nothing and loses the game instead (CR 704.5c) — fires nothing.
    ///
    /// Fires **once per card**, not once per draw effect: a two-card draw is two draw
    /// events as far as any card that watches them is concerned (CR 121.2), so the
    /// recorded count is the fire count rather than a single tick.
    YouDrawCard,
    /// A player **activated an ability** this transition (CR 602.2) matching
    /// `observes`, e.g. `Whenever an opponent activates a nonmana ability of a creature
    /// or land, …`.
    ///
    /// Observed by diffing the **stack**: an activation is a player putting an object
    /// there and paying for it, and [`crate::stack::AbilityOrigin::Activated`] recorded
    /// at that push is what tells it apart from a trigger the game put there or a spell
    /// a player cast. Fires once per new such object.
    ///
    /// **A mana ability never fires this** (CR 605.3a), and that exclusion is part of
    /// the condition rather than something a card has to say: a mana ability resolves
    /// immediately without ever using the stack, so there is no object for this diff to
    /// find. Tapping a land for mana is therefore silent here by construction, which is
    /// the only way it could be reliable — a card-authored filter would have to be
    /// repeated correctly on every card that watches an activation.
    ///
    /// The source must still be on the battlefield after the transition.
    AbilityActivated(
        /// Whose activations, and of what, satisfy this condition.
        ObservedActivation,
    ),
    /// The turn structure **began a step** this transition (CR 603.6a) — the
    /// "at the beginning of your upkeep" / "at the beginning of each end step" shape.
    /// The first condition here that is about the turn rather than about an object.
    ///
    /// Read from the [`crate::GameEvent::StepChanged`] entries the transition
    /// recorded, not by comparing `before.step` to `after.step`, and the distinction
    /// is load-bearing twice over. A snapshot comparison would **miss** steps: one
    /// pass of priority can walk through several steps at once (end → cleanup → untap
    /// → upkeep), and only the last of them would be visible in `after`. It would also
    /// **misfire**: a step recurs every turn, so `after.step == Upkeep` is true of a
    /// great many transitions that crossed no boundary at all. Each crossing records
    /// exactly one event, so counting events is exactly once per crossing — the turn
    /// number never has to enter the comparison because the event *is* the crossing.
    ///
    /// The source must still be on the battlefield after the transition; a permanent
    /// that is gone is not there to trigger.
    BeginningOfStep {
        /// Which step's beginning this watches.
        step: TriggerStep,
        /// Whose turn that step has to belong to.
        whose_turn: TurnScope,
    },
}

/// Which step a [`TriggerCondition::BeginningOfStep`] watches the beginning of.
///
/// Deliberately **not** every [`crate::phase::Step`]: this names the four steps printed
/// cards actually trigger at, all four of which grant priority (CR 117.1). That matters
/// — a trigger owed at a step the turn-structure walk passes straight through
/// (untap, CR 502.5; cleanup, CR 514.3) would reach the stack only after the walk had
/// already left the step behind. Keeping those out of the vocabulary means the
/// condition cannot express a trigger the pipeline would answer in the wrong step.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriggerStep {
    /// The upkeep step (CR 503) — "at the beginning of your upkeep".
    Upkeep,
    /// The draw step (CR 504) — "at the beginning of your draw step".
    Draw,
    /// The beginning of combat step (CR 507) — "at the beginning of combat on your turn".
    BeginCombat,
    /// The end step (CR 513) — "at the beginning of each end step".
    EndStep,
}

impl TriggerStep {
    /// The turn-structure [`Step`](crate::phase::Step) this names.
    #[must_use]
    pub fn step(self) -> crate::phase::Step {
        match self {
            Self::Upkeep => crate::phase::Step::Upkeep,
            Self::Draw => crate::phase::Step::Draw,
            Self::BeginCombat => crate::phase::Step::BeginCombat,
            Self::EndStep => crate::phase::Step::End,
        }
    }
}

/// Whose turn a [`TriggerCondition::BeginningOfStep`] has to be for it to fire.
///
/// This distinction is most of what a step trigger *means*: "at the beginning of your
/// upkeep" happens once every other turn and "at the beginning of each upkeep" happens
/// every turn, and the two are otherwise the same ability. Scoped relative to the
/// source's controller, exactly as [`ObservedPermanent`] and [`StaticAffects`] are.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnScope {
    /// Only on the source controller's own turn — the "your" of "your upkeep".
    Yours,
    /// On every player's turn — the "each" of "each upkeep".
    Each,
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
        /// Notice only permanents that are **cards** — the "nontoken" of "whenever
        /// another nontoken Dragon you control enters". A token has no card behind it
        /// (ADR 0015), which is exactly the distinction this draws, and it is what keeps
        /// a card that makes a token off an infinite loop with its own trigger.
        #[serde(default)]
        nontoken: bool,
        /// Notice only creatures whose power is at most this — the "with **power 2 or
        /// less**" of an enters-the-battlefield watcher. Absent notices every power.
        ///
        /// Read through the **computed** characteristics of the state the observed
        /// event happened in, so a creature that entered pumped is judged by what it
        /// was then, not by its printed face.
        #[serde(default)]
        max_power: Option<i32>,
        /// Notice only creatures that have this keyword — the "with **flying**" of an
        /// attack watcher. Absent notices every creature.
        ///
        /// Read through the **computed** keywords (CR 613 layer 6), like the power
        /// bound beside it and unlike the printed subtype above: the layer that grants
        /// a keyword is implemented, so a creature that was given flying really is one
        /// a flying watcher notices, and really stops being one when the grant ends.
        #[serde(default)]
        keyword: Option<Keyword>,
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
        /// Notice only permanents that are cards, never tokens.
        #[serde(default)]
        nontoken: bool,
        /// Notice only creatures whose power is at most this, read through the computed
        /// characteristics exactly as the sibling variant's is.
        #[serde(default)]
        max_power: Option<i32>,
        /// Notice only creatures with this keyword, read through the computed keywords
        /// exactly as the sibling variant's is.
        #[serde(default)]
        keyword: Option<Keyword>,
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

    /// Whether this selector notices only nontoken permanents (CR 111 — a token is not
    /// a card).
    #[must_use]
    pub fn nontoken_only(&self) -> bool {
        match self {
            ObservedPermanent::CreaturesYouControl { nontoken, .. }
            | ObservedPermanent::AnyCreature { nontoken, .. } => *nontoken,
        }
    }

    /// The upper power bound this selector restricts to, if any.
    #[must_use]
    pub fn max_power(&self) -> Option<i32> {
        match self {
            ObservedPermanent::CreaturesYouControl { max_power, .. }
            | ObservedPermanent::AnyCreature { max_power, .. } => *max_power,
        }
    }

    /// The keyword this selector restricts to, if any (CR 702) — read through the
    /// computed keywords wherever it is evaluated, never off the printed face.
    #[must_use]
    pub fn keyword(&self) -> Option<Keyword> {
        match self {
            ObservedPermanent::CreaturesYouControl { keyword, .. }
            | ObservedPermanent::AnyCreature { keyword, .. } => *keyword,
        }
    }
}

/// Which activations a [`TriggerCondition::AbilityActivated`] notices.
///
/// A small product of two independent filters rather than a closed list of named
/// classes, for the reason [`crate::PermanentCount`] is one: the two questions a card
/// asks about an activation — *whose* it was, and what kind of permanent it was an
/// ability *of* — vary independently, and a variant per pairing would grow the
/// vocabulary once per card.
///
/// Nothing here says "that isn't a mana ability": a mana ability never uses the stack
/// (CR 605.3a), and the stack is where this condition looks, so the exclusion is
/// structural. See [`TriggerCondition::AbilityActivated`].
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct ObservedActivation {
    /// Whose activations are noticed, relative to the watching ability's controller.
    /// Defaults to every player's.
    #[serde(default)]
    pub activator: ActivatorScope,
    /// The permanent types whose abilities are noticed — `["creature", "land"]` for the
    /// "of a creature or land" of a printed watcher, satisfied by a permanent with
    /// **any** of them. Empty (the default) notices every permanent's.
    ///
    /// Read off the printed face, like every other type question the selectors ask: the
    /// layer that would change a permanent's types is not implemented.
    #[serde(default)]
    pub source_types: Vec<CardType>,
}

/// Whose activation an [`ObservedActivation`] notices, relative to the watching
/// ability's controller.
///
/// Scoped exactly as [`ObservedPermanent`] and [`TurnScope`] are — the watcher's own
/// controller is the "you" every scope here is written from. The activator is the
/// activated permanent's controller, since only a permanent's controller may activate
/// its abilities (CR 602.1a).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivatorScope {
    /// Any player's activation, the watcher's own controller included.
    #[default]
    Any,
    /// Only an opponent of the watcher's controller — the "whenever an opponent
    /// activates" of a punisher.
    Opponents,
}

/// Which spells a [`TriggerCondition::YouCastSpell`] notices, and which spells an
/// [`Ability::CostModifier`] changes the cost of.
///
/// One spell vocabulary, shared by the two abilities that name a class of spells,
/// because they ask the same question of the same object: *is this spell one of those?*
/// Nothing here reads the stack — a cost modification is judged while the card is still
/// in the zone it is being cast from — so the predicate is about the **card**, which is
/// what makes one answer serve both.
///
/// A closed set deserialized in serde's **externally tagged** form: the classes that
/// carry no parameter are bare `snake_case` strings (`"enchantment"`) and one that does
/// wraps its filter (`{"creature": {"min_power": 4}}`), exactly as
/// [`TriggerCondition`] itself is authored. Deliberately named by the classes real cards
/// ask about rather than by card type, because "instant or sorcery" is one class to a
/// card and two types to the engine.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservedSpell {
    /// An enchantment spell.
    Enchantment,
    /// An **artifact spell** — including an artifact creature, which is an artifact spell
    /// and a creature spell at once (CR 205.2b). Read off the printed types like every
    /// other class here, so a card that is both satisfies both.
    Artifact,
    /// A **creature spell**, optionally only one whose power is at least `min_power` —
    /// the "creature spell with power 4 or greater" a cost reducer names.
    ///
    /// The power is the card's **printed** power, and that is the only reading available
    /// rather than a simplification: the class is asked about a card in a hand, a
    /// graveyard, or a command zone, which has no [`crate::PermanentId`], no controller,
    /// and no computed characteristics of any kind. It is the same printed reading
    /// [`CardFilter::Creature`] takes of a card in a pile, and the opposite of the
    /// computed one [`ObservedPermanent::max_power`] takes of a permanent on the
    /// battlefield — the difference is what the object *is*, not a choice either
    /// selector made.
    ///
    /// Only a lower bound exists, because only a lower bound is printed on a card the
    /// catalog defines; an upper one arrives with the card that needs it.
    Creature {
        /// The least printed power a matching creature spell may have. Absent matches
        /// every creature spell, including one with no printed power at all.
        #[serde(default)]
        min_power: Option<i32>,
    },
    /// An instant **or** sorcery spell — one class, as a card writes it.
    InstantOrSorcery,
    /// A spell of the **chosen color** — the class a card names *after* its controller
    /// has answered the choice made as it entered
    /// ([`Ability::EntersChoosingColor`](crate::Ability)).
    ///
    /// The one member of this set whose meaning is not fixed by the card: it is read
    /// off [`Permanent::chosen_color`](crate::Permanent) at the moment the cast is
    /// observed, so the same printed ability watches a different class on two
    /// battlefields. A source with no colour recorded — a token given the ability, or a
    /// permanent whose card never declared the choice — notices nothing at all, which is
    /// the honest answer to "spells of which colour?" when none was named.
    ///
    /// Satisfied by a spell whose printed colours *include* the chosen one (CR 105.2),
    /// so a gold spell is of each of its colours and a colourless spell is of none.
    ChosenColor,
}
