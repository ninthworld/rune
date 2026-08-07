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
    /// **Every player still in the game**, the controller included — the "each player"
    /// of a symmetric sweeper. Chooses no target, and it is a separate variant from
    /// [`Self::EachOpponent`] rather than a flag on it because the difference is the
    /// whole point of the cards that print it: a spell that makes each player discard
    /// half their hand makes its caster discard too.
    EachPlayer,
    /// The player this resolution's most recent targeted effect **named** — the `its
    /// controller` of `Destroy target creature. Its controller creates a 2/4 white Ox`.
    ///
    /// Not a target and never a slot: the choice was made by the sentence before it, and a
    /// slot here would be a second choice the card does not ask for. Read from
    /// [`Resolution::chosen_player`](crate::Resolution), which is written before the
    /// naming effect is applied — so a creature that is destroyed still had a controller
    /// when the question was asked (CR 608.2h).
    ///
    /// Names nobody in a resolution that has aimed at nothing, which is a card that could
    /// not have been written: the phrase only exists after a sentence that chose.
    ThatPlayer,
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

    /// A targeted opponent — the serde default for an effect that only ever reaches across
    /// the table ([`Effect::ExileFromLibraryUntil`](crate::Effect)).
    pub(super) fn target_opponent() -> Self {
        Self::TargetOpponent
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
            // "That player" fills no slot for the same reason "you" does not: the sentence
            // names them rather than asking anybody to choose.
            PlayerRef::Controller
            | PlayerRef::EachOpponent
            | PlayerRef::EachPlayer
            | PlayerRef::ThatPlayer => None,
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
/// - **Includes them by construction**: [`Self::AnyPermanent`],
///   [`Self::AnyNonlandPermanent`], and [`Self::AnyPermanentWithManaValue`] name
///   permanents, and a planeswalker is one — the first two already accepted one before
///   this issue and still do.
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
    /// Any permanent controlled by the player in **seat `seat`** — "target permanent
    /// that player controls", once "that player" has been fixed to one seat.
    ///
    /// The one spec that names an **absolute** seat rather than a class relative to the
    /// object's controller, and the only one no card authors directly: a card that says
    /// `for each player, choose target permanent that player controls` declares one group
    /// per seat, and each group carries the seat it belongs to.
    /// [`Effect::target_groups`](crate::Effect::target_groups) builds them from the seat
    /// count, which is why it is given one.
    ///
    /// It is absolute because it has to be. Every other spec answers "whose?" with *you*
    /// or *an opponent*, and both are relative to whoever is choosing — but these groups
    /// are all chosen by one player and each names a different seat, so the seat cannot
    /// come from the chooser. Storing it on the spec is also what makes the CR 608.2b
    /// re-check right: a permanent that changed controllers between announcement and
    /// resolution is no longer a legal target for the group it was announced for, which
    /// is exactly what the printed card means.
    ///
    /// **Includes a planeswalker**, for [`Self::AnyPermanent`]'s reason.
    PermanentThatPlayerControls {
        /// The seat whose permanents this slot may name — an index into
        /// [`GameState::players`](crate::GameState).
        seat: usize,
    },
    /// Any permanent that is not a land — "target nonland permanent". **Includes a
    /// planeswalker**, which is a permanent and is not a land.
    AnyNonlandPermanent,
    /// Any nonland permanent controlled by an opponent of the object's controller —
    /// "target nonland permanent an opponent controls", the one-sided removal a colorless
    /// creature's enters-the-battlefield trigger names.
    ///
    /// **Includes a planeswalker**, for [`Self::AnyNonlandPermanent`]'s reason: it is a
    /// permanent and it is not a land. The controller's own permanents are never
    /// candidates, which is the whole difference between this spec and that one.
    AnyNonlandPermanentAnOpponentControls,
    /// Any permanent whose mana value (CR 202.3) is **exactly** `mana_value` — "target
    /// permanent with mana value 1".
    ///
    /// The first spec that filters by a *number off the printed face* rather than by a
    /// type, a controller, or a keyword. It is an equality rather than the
    /// [`Self::CardInGraveyard`] cap for the reason the printed cards differ: a cap
    /// admits everything cheaper, and a card that names one value means that value.
    /// The two therefore stay separate fields on separate specs rather than one shared
    /// bound that would have to say which comparison it meant.
    ///
    /// The value is read through [`PrintedFace::mana_value`](crate::PrintedFace), so a
    /// **token** answers `0` (CR 111.4 — no mana cost) rather than being skipped: a
    /// token is a permanent, and a spell that names mana value 1 misses it because of
    /// what it is worth, not because it has no card. **Includes a planeswalker**, for
    /// [`Self::AnyPermanent`]'s reason.
    AnyPermanentWithManaValue {
        /// The mana value a candidate must have.
        mana_value: u32,
    },
    /// Any creature on the battlefield (a permanent whose printed types include
    /// [`crate::CardType::Creature`]). Never a planeswalker: the two types are
    /// disjoint on every card the schema can express, and a spell that wants both says
    /// [`Self::AnyTarget`].
    AnyCreature,
    /// Any **colorless** creature (CR 105.2) — "target colorless creature", the class a
    /// one-mana answer to an Eldrazi names.
    ///
    /// Colour is read off the card's printed
    /// [`colors`](crate::CardData::colors), which is where every other colour test in the
    /// engine reads it: colour-changing effects are not modelled, so printed colour is
    /// current colour here exactly as it is in the blocking restriction that names one.
    /// A token contributes the colours its creating effect gave it. Never a planeswalker
    /// (see [`Self::AnyCreature`]).
    AnyColorlessCreature,
    /// Any creature the object's controller controls — "target creature you control".
    /// Never a planeswalker (see [`Self::AnyCreature`]).
    AnyCreatureYouControl,
    /// Any creature controlled by an opponent of the object's controller — "target
    /// creature an opponent controls". Never a planeswalker (see [`Self::AnyCreature`]).
    AnyCreatureAnOpponentControls,
    /// A creature the object's controller controls, **other than the ability's own
    /// source** — "another target creature you control".
    ///
    /// Source-relative like [`Self::AnotherAttackingCreature`], and "another" means the
    /// same thing here: the source permanent excluded by
    /// [`PermanentId`](crate::PermanentId), so two copies of one card each name the
    /// other. Names nothing at all for a spell, which has no source permanent to be
    /// another one than.
    AnotherCreatureYouControl,
    /// Any creature that currently has flying (CR 613.1f, so a granted flying counts
    /// exactly as a printed one) — the "target creature with flying" of an anti-air
    /// removal spell. Never a planeswalker (see [`Self::AnyCreature`]).
    AnyCreatureWithFlying,
    /// Any **artifact creature** the object's controller controls — "target artifact
    /// creature you control". Both types are read off the printed card, the way every
    /// other type test in the engine is. Never a planeswalker (see
    /// [`Self::AnyCreature`]).
    AnyArtifactCreatureYouControl,
    /// Any creature that is currently tapped — "target tapped creature". Never a
    /// planeswalker: a planeswalker can be tapped in principle, but this spec is a
    /// creature spec first and the tapped-ness is a filter on it.
    AnyTappedCreature,
    /// An **attacking** creature other than the ability's own source — the "another
    /// target attacking creature" of a card that helps the rest of the team through.
    ///
    /// The first spec relative to the *source* rather than to its controller, and both
    /// halves of it are: "attacking" is read off the declaration
    /// ([`Permanent::attacking`](crate::Permanent)), and "another" is the source itself
    /// excluded by [`PermanentId`](crate::PermanentId) — so two copies of one card each
    /// name the other, and a permanent never names itself.
    ///
    /// It names nothing at all when there is no source: a spell that said this would be
    /// asking about a permanent it does not have.
    AnotherAttackingCreature,
    /// A creature controlled by the player the ability's source is **attacking** — the
    /// "target creature defending player controls" of an attack trigger.
    ///
    /// The defending player is read off the source's own attack (CR 508.1a, and
    /// [`AttackTarget::defending_player`](crate::combat::AttackTarget) for an attack
    /// aimed at a planeswalker, whose controller is the defending player). A source that
    /// is not attacking names nobody, which is the honest answer between combats: the
    /// phrase has no meaning outside one.
    AnyCreatureDefendingPlayerControls,
    /// Any artifact on the battlefield. Never a planeswalker — no printing in the
    /// bundled catalog is both, and a card that wants either says so with its own spec.
    AnyArtifact,
    /// Any artifact the object's controller controls — "target artifact you control", the
    /// class an animation spell names. Not restricted to artifact *creatures*
    /// ([`Self::AnyArtifactCreatureYouControl`]), which is the whole point of a card that
    /// turns one into a creature.
    AnyArtifactYouControl,
    /// Any enchantment on the battlefield. Never a planeswalker, for
    /// [`Self::AnyArtifact`]'s reason.
    AnyEnchantment,
    /// Any artifact **or** enchantment — the single slot of a naturalize-style
    /// spell, which is one target of either type rather than two slots. Never a
    /// planeswalker.
    AnyArtifactOrEnchantment,
    /// Any land on the battlefield. Never a planeswalker: the types are disjoint.
    AnyLand,
    /// Any **creature or planeswalker** on the battlefield — the single slot of a
    /// removal spell that names both classes in one breath.
    ///
    /// One spec rather than two groups, for the reason
    /// [`Self::AnyArtifactOrEnchantment`] is one: the printed sentence names one target
    /// of either type, and two groups would advertise two slots and let a player aim at
    /// a creature *and* a planeswalker. It is the one spec whose candidates span the two
    /// classes damage treats differently (CR 120.3c — marked on a creature, taken off a
    /// planeswalker's loyalty), which is decided where the damage is dealt and not here.
    AnyCreatureOrPlaneswalker,
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
    /// A **card in a graveyard** — `target creature card with mana value 2 or less from
    /// your graveyard`, `target instant or sorcery card from your graveyard`, `target
    /// creature card from a graveyard`.
    ///
    /// The only spec that names a card in a zone rather than an object on the
    /// battlefield or the stack, so it is the only one a [`Target::Card`] satisfies.
    /// A graveyard is public, so there is no hidden information to protect and the
    /// candidate set is enumerable exactly as a battlefield one is.
    ///
    /// Its three fields are the three independent things a printed card says about such
    /// a target — *whose* graveyard, *what kind* of card, and *how expensive* — rather
    /// than one variant per phrasing. The class is a small `Copy` enum of its own rather
    /// than the [`CardFilter`] the mid-resolution choices use, because [`TargetSpec`] is
    /// `Copy` and threaded by value through every targeting path; a filter carrying a
    /// subtype string would take that away from all of them to serve no card here.
    CardInGraveyard {
        /// Whose graveyards are candidates. Defaults to the object controller's own,
        /// which is what nearly every card printed this way says.
        #[serde(default)]
        scope: GraveyardScope,
        /// Which class of card qualifies. Defaults to any of them.
        #[serde(default)]
        class: GraveyardCardClass,
        /// The greatest mana value (CR 202.3) a matching card may have. Absent means
        /// any.
        #[serde(default)]
        max_mana_value: Option<u32>,
        /// The **exact** mana value a matching card must have. Absent means any.
        ///
        /// Separate from [`max_mana_value`](Self::CardInGraveyard::max_mana_value)
        /// because the printed cards differ: a cap admits everything cheaper, and a card
        /// that says `with mana value X` means that value and no other. Collapsing them
        /// into one bound would need a second field saying which comparison it meant,
        /// which is the field this is.
        ///
        /// It is the one part of a spec that no card **authors**: it arrives from an X
        /// its controller paid mid-resolution, substituted into the reflexive ability
        /// that reads it ([`OptionalCost::ManaX`](crate::OptionalCost)). Authoring it
        /// directly is legal and means what it says.
        #[serde(default)]
        exact_mana_value: Option<u32>,
        /// Whether the exact mana value is **the X this ability's controller paid** —
        /// the `with mana value X` of a sentence that follows a `you may pay {X}`.
        ///
        /// This is what a card authors, and it is a marker rather than a number because
        /// at authoring time there is no number: X does not exist until a player names
        /// it. The moment they do, the value is substituted in — this becomes `false` and
        /// [`exact_mana_value`](Self::CardInGraveyard::exact_mana_value) becomes
        /// `Some(x)` — so every later reader sees an ordinary, concrete spec and nothing
        /// downstream has to know an X was ever involved.
        ///
        /// A spec still carrying it has not been substituted into, which can only mean it
        /// was authored somewhere no X is paid; it names nothing, because "the X that was
        /// paid" is not a number when nothing was paid.
        #[serde(default)]
        mana_value_is_x: bool,
    },
}

/// Whose graveyards a [`TargetSpec::CardInGraveyard`] draws candidates from.
///
/// The difference between "from your graveyard" and "from a graveyard" is the whole
/// content of a reanimation spell's colour pie position, so it is a field rather than an
/// assumption.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GraveyardScope {
    /// The object controller's own graveyard — "from your graveyard".
    #[default]
    Yours,
    /// Every player's graveyard — "from a graveyard".
    Any,
}

/// Which class of card a [`TargetSpec::CardInGraveyard`] accepts, read off the card's
/// **printed** types: a card in a graveyard is not on the battlefield, so it has no
/// computed characteristics to read instead.
///
/// Deliberately a closed list of the classes printed cards name, and deliberately `Copy`
/// — see [`TargetSpec::CardInGraveyard`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GraveyardCardClass {
    /// Any card at all — "target card in your graveyard".
    #[default]
    Any,
    /// A creature card.
    Creature,
    /// A creature **or planeswalker** card — the single class of a reanimation that
    /// names both, rather than two classes one target could not be in at once.
    CreatureOrPlaneswalker,
    /// An instant **or** sorcery card — one class as a card writes it, not two types.
    InstantOrSorcery,
    /// An artifact card.
    Artifact,
    /// A land card.
    Land,
}

impl GraveyardCardClass {
    /// Whether `data`'s printed types put it in this class.
    #[must_use]
    pub fn matches(self, data: &crate::CardData) -> bool {
        match self {
            GraveyardCardClass::Any => true,
            GraveyardCardClass::Creature => data.has_type(CardType::Creature),
            GraveyardCardClass::CreatureOrPlaneswalker => {
                data.has_type(CardType::Creature) || data.has_type(CardType::Planeswalker)
            }
            GraveyardCardClass::InstantOrSorcery => {
                data.has_type(CardType::Instant) || data.has_type(CardType::Sorcery)
            }
            GraveyardCardClass::Artifact => data.has_type(CardType::Artifact),
            GraveyardCardClass::Land => data.has_type(CardType::Land),
        }
    }
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

/// What kind of object a [`Target`] names — the one structural fact about a target that
/// never changes while it is chosen.
///
/// The pairing of an object's flat target list back onto the groups that were announced
/// ([`group_target_counts`]) is done by this and nothing else, deliberately. Legality
/// cannot be used for it: a target that has become illegal must still pair with the group
/// it was chosen for, or the CR 608.2b fizzle check would be asking about the wrong slot.
/// What a target *is* survives everything that could happen to it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TargetKind {
    /// A seat.
    Player,
    /// A permanent on the battlefield.
    Permanent,
    /// A card in a zone that is not the battlefield.
    Card,
    /// An object on the stack.
    Spell,
}

impl Target {
    /// What kind of object this target names.
    #[must_use]
    pub fn kind(self) -> TargetKind {
        match self {
            Target::Player(_) => TargetKind::Player,
            Target::Permanent(_) => TargetKind::Permanent,
            Target::Card(_) => TargetKind::Card,
            Target::Spell(_) => TargetKind::Spell,
        }
    }
}

impl TargetSpec {
    /// Whether a target of `kind` could have been chosen for this spec.
    ///
    /// A structural question, not a legality one — "could this slot ever hold a
    /// permanent", never "is this permanent a legal choice right now". Two specs name
    /// **two** kinds apiece and are the reason this returns a predicate rather than one
    /// kind: `any target` is CR 115.4's creature, player, or planeswalker, and `any player
    /// or planeswalker` names its two outright.
    ///
    /// Exhaustive with no wildcard, so a new spec has to say what it names.
    #[must_use]
    pub fn names(self, kind: TargetKind) -> bool {
        match self {
            // The player specs, and the two that name a player *or* a permanent.
            Self::AnyPlayer | Self::AnyOpponent => kind == TargetKind::Player,
            Self::AnyPlayerOrPlaneswalker | Self::AnyTarget => {
                matches!(kind, TargetKind::Player | TargetKind::Permanent)
            }
            // A card in a graveyard is the one spec that names a card rather than an
            // object on the battlefield.
            Self::CardInGraveyard { .. } => kind == TargetKind::Card,
            // The two that name an object on the stack.
            Self::SpellOnStack | Self::CreatureSpellOnStack => kind == TargetKind::Spell,
            // Everything else names a permanent.
            Self::AnyPermanent
            | Self::PermanentThatPlayerControls { .. }
            | Self::AnyNonlandPermanent
            | Self::AnyNonlandPermanentAnOpponentControls
            | Self::AnyPermanentWithManaValue { .. }
            | Self::AnyCreature
            | Self::AnyColorlessCreature
            | Self::AnyCreatureYouControl
            | Self::AnyCreatureAnOpponentControls
            | Self::AnotherCreatureYouControl
            | Self::AnyCreatureWithFlying
            | Self::AnyArtifactCreatureYouControl
            | Self::AnyTappedCreature
            | Self::AnotherAttackingCreature
            | Self::AnyCreatureDefendingPlayerControls
            | Self::AnyArtifact
            | Self::AnyArtifactYouControl
            | Self::AnyEnchantment
            | Self::AnyArtifactOrEnchantment
            | Self::AnyLand
            | Self::AnyCreatureOrPlaneswalker
            | Self::AnyArtifactEnchantmentOrCreatureWithFlying => kind == TargetKind::Permanent,
        }
    }

    /// Whether this spec and `other` could ever name the same object — the test that
    /// decides whether two variable-arity groups on one object can be told apart.
    #[must_use]
    pub fn overlaps(self, other: Self) -> bool {
        [
            TargetKind::Player,
            TargetKind::Permanent,
            TargetKind::Card,
            TargetKind::Spell,
        ]
        .into_iter()
        .any(|kind| self.names(kind) && other.names(kind))
    }
}

/// The **fewest** targets a legal announcement of `effects` must choose (CR 601.2c) —
/// the sum of every declared group's minimum.
///
/// Equal to [`maximum_targets`] for every object but one that declares an "up to N"
/// group, which is the whole reason the two are separate functions.
#[must_use]
pub fn minimum_targets(effects: &[Effect], seats: usize) -> usize {
    effects
        .iter()
        .flat_map(|effect| effect.target_groups(seats))
        .map(|group| usize::from(group.min))
        .sum()
}

/// The **most** targets a legal announcement of `effects` may choose (CR 601.2c) — the
/// sum of every declared group's maximum.
#[must_use]
pub fn maximum_targets(effects: &[Effect], seats: usize) -> usize {
    effects
        .iter()
        .flat_map(|effect| effect.target_groups(seats))
        .map(|group| usize::from(group.max))
        .sum()
}

/// How many of an object's `chosen` targets each of `effects`' target groups consumes,
/// in effect order — the pairing every path that walks stored targets alongside effects
/// needs (announcement, the CR 608.2b resolution re-check, the legality gate).
///
/// Each chosen target goes to the **first group that could have named it** and is not
/// already full ([`TargetSpec::names`]). That is exact rather than a guess for the two
/// shapes an object can have:
///
/// - one variable-arity group, however many fixed ones: the slack can only belong to the
///   one group that has any, so kinds never have to be compared;
/// - two variable groups whose specs name **different kinds** of object — `destroy up to
///   two target creatures. Put up to two creature cards from graveyards onto the
///   battlefield` — where a permanent can only be the first group's and a card can only
///   be the second's.
///
/// Two variable groups that name the *same* kind would be genuinely ambiguous, and the
/// catalog validator refuses them
/// ([`Violation::TwoVariableTargetGroups`](crate::Violation)) for that reason and no
/// other.
///
/// Pairing on kind rather than on legality is what makes this survive a resolution: a
/// target that has become illegal must still pair with the group it was announced for, or
/// the CR 608.2b re-check would be asking about the wrong slot.
#[must_use]
pub fn target_counts(effects: &[Effect], chosen: &[Target], seats: usize) -> Vec<usize> {
    let groups: Vec<TargetGroup> = effects
        .iter()
        .flat_map(|effect| effect.target_groups(seats))
        .collect();
    group_target_counts(&groups, chosen)
}

/// [`target_counts`] over groups a caller already has in hand.
#[must_use]
pub fn group_target_counts(groups: &[TargetGroup], chosen: &[Target]) -> Vec<usize> {
    let mut counts = vec![0usize; groups.len()];
    for target in chosen {
        let kind = target.kind();
        let slot = groups.iter().enumerate().find(|(index, group)| {
            counts[*index] < usize::from(group.max) && group.spec.names(kind)
        });
        // A target no group could have named is left unpaired: it is one the announcement
        // should never have accepted, and dropping it here is what keeps it from silently
        // shifting every slot after it.
        if let Some((index, _)) = slot {
            counts[index] += 1;
        }
    }
    counts
}
