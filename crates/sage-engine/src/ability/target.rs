//! Targeting (CR 115, CR 601.2c): what a slot may be pointed at, how many slots an
//! effect takes, and the chosen values that come back.

use super::*;

/// How many targets one [`Effect`]'s slot group takes (CR 601.2c).
///
/// Almost every effect in the IR takes exactly one target, and says so by leaving this
/// at its default. The exception is the "up to N target …" shape, where the *player*
/// decides how many of the slots to fill — including none — which is a fact about the
/// effect rather than about any one slot, so it lives here.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetCount {
    /// Exactly this many targets, all of which must be chosen and legal.
    Exactly(u8),
    /// **Up to** this many — the player may choose fewer, or none at all, and the
    /// effect applies once per target actually chosen.
    UpTo(u8),
}

impl Default for TargetCount {
    /// One target, the shape every effect but the "up to N" one has.
    fn default() -> Self {
        Self::Exactly(1)
    }
}

/// One [`Effect`]'s target requirement: what it may target, and how many of them.
///
/// The unit the whole targeting pipeline works in since a variable-arity effect
/// exists — announcement (CR 601.2c), the per-slot candidate enumeration
/// ([`crate::target_requirements`]), the legality gate, and the CR 608.2b resolution
/// re-check. A group with `min == max == 1` is the ordinary single-target effect and
/// behaves exactly as it always did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TargetGroup {
    /// What each of this group's slots may target.
    pub spec: TargetSpec,
    /// The fewest targets a legal announcement may choose. `0` for an "up to N".
    pub min: u8,
    /// The most it may choose.
    pub max: u8,
}

impl TargetGroup {
    /// The ordinary one-target group.
    pub(super) fn single(spec: TargetSpec) -> Self {
        Self {
            spec,
            min: 1,
            max: 1,
        }
    }

    /// The group `count` describes for `spec`.
    pub(super) fn counted(spec: TargetSpec, count: TargetCount) -> Self {
        match count {
            TargetCount::Exactly(n) => Self {
                spec,
                min: n,
                max: n,
            },
            TargetCount::UpTo(n) => Self {
                spec,
                min: 0,
                max: n,
            },
        }
    }
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
    /// The controller of the effect — the serde default for a reference that is
    /// almost always "you" ([`Effect::CreateToken`]).
    pub(super) fn controller() -> Self {
        Self::Controller
    }

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
///
/// # Where each spec stands on planeswalkers
///
/// Now that a planeswalker is a real object (issue #608), every spec has to have an
/// answer, and the compiler cannot ask for one: a spec that names a *type* excludes
/// planeswalkers by saying nothing, which is indistinguishable from having forgotten
/// them. So each variant states its position in its own doc comment below, and the
/// three groups are:
///
/// - **Includes them by construction**: [`Self::AnyPermanent`] and
///   [`Self::AnyNonlandPermanent`] name permanents, and a planeswalker is one — both
///   already accepted one before this issue and still do.
/// - **Includes them by rule**: [`Self::AnyTarget`], which is CR 115.4's "any target"
///   and therefore *means* creature, player, or planeswalker; and
///   [`Self::AnyPlayerOrPlaneswalker`], which names them outright.
/// - **Excludes them**, deliberately and permanently: every creature-shaped spec, the
///   artifact/enchantment/land specs, the player specs, and the two spell-on-stack
///   specs. A planeswalker is not a creature, an artifact, an enchantment, a land, a
///   player, or a spell, so each of these excludes it because of what it names — not
///   because planeswalkers were unmodeled.
pub enum TargetSpec {
    /// Any player in the game. Never a planeswalker: a planeswalker is a permanent, and
    /// the player who controls one is a separate object (CR 306.1).
    AnyPlayer,
    /// Any player in the game **or** any planeswalker on the battlefield — the
    /// "target player or planeswalker" a burn spell that cannot hit creatures names
    /// (`Lava Axe`, `Viashino Pyromancer`).
    ///
    /// Not [`Self::AnyTarget`] with a hole in it: CR 115.4's "any target" is
    /// creature-or-planeswalker-or-player, and this class deliberately excludes
    /// creatures, so a spell written this way can never be pointed at one. Not
    /// [`Self::AnyPlayer`] either — before this spec existed these cards used it, and
    /// the planeswalker half of what they say was silently missing.
    AnyPlayerOrPlaneswalker,
    /// Any **opponent** of the object's controller still in the game — "target
    /// opponent". The controller themselves is never a candidate, and neither is a
    /// planeswalker, for [`Self::AnyPlayer`]'s reason.
    AnyOpponent,
    /// Any permanent on the battlefield — **including a planeswalker**, which has been
    /// true since the day this variant existed and is now reachable rather than
    /// theoretical.
    AnyPermanent,
    /// Any permanent that is not a land — "target nonland permanent". **Includes a
    /// planeswalker**, which is a permanent and is not a land.
    AnyNonlandPermanent,
    /// Any creature on the battlefield (a permanent whose printed types include
    /// [`crate::CardType::Creature`]). Never a planeswalker: the two types are
    /// disjoint on every card the schema can express, and a spell that wants both says
    /// [`Self::AnyTarget`].
    AnyCreature,
    /// Any creature the object's controller controls — "target creature you control".
    /// Never a planeswalker (see [`Self::AnyCreature`]).
    AnyCreatureYouControl,
    /// Any creature controlled by an opponent of the object's controller — "target
    /// creature an opponent controls". Never a planeswalker (see [`Self::AnyCreature`]).
    AnyCreatureAnOpponentControls,
    /// Any creature that currently has flying (CR 613.1f, so a granted flying counts
    /// exactly as a printed one) — the "target creature with flying" of an anti-air
    /// removal spell. Never a planeswalker (see [`Self::AnyCreature`]).
    AnyCreatureWithFlying,
    /// Any creature that is currently tapped — "target tapped creature". Never a
    /// planeswalker: a planeswalker can be tapped in principle, but this spec is a
    /// creature spec first and the tapped-ness is a filter on it.
    AnyTappedCreature,
    /// Any artifact on the battlefield. Never a planeswalker — no printing in the
    /// bundled catalog is both, and a card that wants either says so with its own spec.
    AnyArtifact,
    /// Any enchantment on the battlefield. Never a planeswalker, for
    /// [`Self::AnyArtifact`]'s reason.
    AnyEnchantment,
    /// Any artifact **or** enchantment — the single slot of a naturalize-style
    /// spell, which is one target of either type rather than two slots. Never a
    /// planeswalker.
    AnyArtifactOrEnchantment,
    /// Any land on the battlefield. Never a planeswalker: the types are disjoint.
    AnyLand,
    /// Any spell on the stack — a [`crate::StackObjectKind::Spell`] object (CR
    /// 701.5, "counter target spell"). Abilities on the stack are not spells and
    /// are never candidates; a mana ability never uses the stack at all (CR
    /// 605.3), so it can never be countered. A planeswalker *spell* on the stack is a
    /// candidate here — but a planeswalker *permanent* is not on the stack at all, so
    /// nothing about planeswalkers is special-cased.
    SpellOnStack,
    /// Any **creature** spell on the stack — the narrower counterspell of
    /// `Essence Scatter`. A creature spell is a spell whose card's printed types
    /// include creature; the check is on the card on the stack, not on any
    /// permanent, because it has not entered the battlefield yet. A planeswalker spell
    /// is not a creature spell.
    CreatureSpellOnStack,
    /// Any target (CR 115.4): the modern "any target" of a burn spell — any creature on
    /// the battlefield, any **planeswalker** on the battlefield, or any player still in
    /// the game.
    ///
    /// The planeswalker arm is the point of issue #608's targeting half. Before it,
    /// this variant documented its own gap in prose ("planeswalkers and battles are not
    /// modeled, so the legal set is exactly creatures plus players"), and closing that
    /// gap is what lets a burn spell kill a planeswalker — damage to which removes
    /// loyalty (CR 120.3c) rather than being marked. Battles remain unmodeled and are
    /// still absent from the set.
    AnyTarget,
    /// Any artifact, enchantment, **or creature with flying** — the single slot of
    /// Vivien Reid's `-3`, which is one target of any of three classes rather than
    /// three slots. Flying is read through the computed keywords (CR 613.1f), exactly
    /// as [`Self::AnyCreatureWithFlying`] reads it. Never a planeswalker: none of the
    /// three classes it names is one.
    AnyArtifactEnchantmentOrCreatureWithFlying,
    /// A **creature card in the object controller's graveyard**, optionally capped at a
    /// mana value — `target creature card with mana value 2 or less from your
    /// graveyard`.
    ///
    /// The only spec that names a card in a zone rather than an object on the
    /// battlefield or the stack, so it is the only one a [`Target::Card`] satisfies.
    /// A graveyard is public, so there is no hidden information to protect and the
    /// candidate set is enumerable exactly as a battlefield one is. Never a
    /// planeswalker: it names a *creature* card, and the two types are disjoint on
    /// every card the schema can express.
    CreatureCardInYourGraveyard {
        /// The greatest mana value (CR 202.3) a matching card may have. Absent means
        /// any.
        max_mana_value: Option<u32>,
    },
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

/// The **fewest** targets a legal announcement of `effects` must choose (CR 601.2c) —
/// the sum of every declared group's minimum.
///
/// Equal to [`maximum_targets`] for every object but one that declares an "up to N"
/// group, which is the whole reason the two are separate functions.
#[must_use]
pub fn minimum_targets(effects: &[Effect]) -> usize {
    effects
        .iter()
        .filter_map(Effect::target_group)
        .map(|group| usize::from(group.min))
        .sum()
}

/// The **most** targets a legal announcement of `effects` may choose (CR 601.2c) — the
/// sum of every declared group's maximum.
#[must_use]
pub fn maximum_targets(effects: &[Effect]) -> usize {
    effects
        .iter()
        .filter_map(Effect::target_group)
        .map(|group| usize::from(group.max))
        .sum()
}

/// How many of an object's `chosen` targets each of `effects`' target groups consumes,
/// in effect order — the pairing every path that walks stored targets alongside effects
/// needs (announcement, the CR 608.2b resolution re-check, the legality gate).
///
/// Fixed groups take exactly their size. The slack — the targets chosen beyond every
/// group's minimum — all belongs to the **one** variable-arity group an object may
/// declare, a limit the catalog validator enforces
/// ([`Violation::TwoVariableTargetGroups`](crate::Violation)) precisely so this pairing
/// is exact rather than a guess. With no variable group the slack is zero and every
/// group takes its fixed size, which is what every object authored before "up to two
/// target creatures" existed does.
#[must_use]
pub fn target_counts(effects: &[Effect], chosen: usize) -> Vec<usize> {
    let groups: Vec<TargetGroup> = effects.iter().filter_map(Effect::target_group).collect();
    group_target_counts(&groups, chosen)
}

/// [`target_counts`] over groups a caller already has in hand.
#[must_use]
pub fn group_target_counts(groups: &[TargetGroup], chosen: usize) -> Vec<usize> {
    let minimum: usize = groups.iter().map(|g| usize::from(g.min)).sum();
    let mut slack = chosen.saturating_sub(minimum);
    groups
        .iter()
        .map(|group| {
            let extra = slack.min(usize::from(group.max) - usize::from(group.min));
            slack -= extra;
            usize::from(group.min) + extra
        })
        .collect()
}
