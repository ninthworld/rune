//! What an [`Effect`] *names* without targeting it: the condition it is gated on, the
//! cards it filters for, and the classes of permanent or player it applies to.

use super::*;

/// What has to be true for an [`Effect::Conditional`] to take its `then` branch.
///
/// A closed, plain-data predicate evaluated against the state as the conditional is
/// reached, deliberately separate from [`TargetSpec`] and [`CardFilter`]: those select
/// *objects*, this answers a *question about the game*. Most of them are about what has
/// already *happened* rather than about a board position, which is the whole reason a
/// condition is a thing rather than an inline count.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Condition {
    /// The effect's controller controls at least `count` permanents matching
    /// `permanents` — `if you control three or more artifacts`.
    ControlsAtLeast {
        /// Which permanents are counted, relative to the effect's controller.
        permanents: PermanentCount,
        /// The threshold, inclusive.
        count: u32,
    },
    /// At least one card matching `filter` was **milled by this resolution** — the
    /// `if at least one Zombie card was milled this way` of a self-mill payoff.
    ///
    /// Read from the [`GameEvent::CardsMilled`](crate::GameEvent) entries recorded
    /// since this object began resolving, not from the graveyard: a Zombie that was
    /// already there was not milled this way, and a graveyard scan could never tell the
    /// two apart. The window survives a suspension, so a mill that stops to ask a
    /// question still answers this correctly when it resumes.
    MilledThisWay {
        /// Which milled cards satisfy it. Defaults to any of them.
        #[serde(default)]
        filter: CardFilter,
    },
    /// The effect's controller **discarded a card during this resolution** — the `if a
    /// card is discarded this way` that stops a discard-then-draw from drawing off an
    /// empty hand. Read from the recorded events over the same window
    /// [`Self::MilledThisWay`] uses.
    DiscardedThisWay,
    /// The effect's controller has **gained at least `amount` life this turn** — the
    /// `if you gained life this turn` of a Bat-making end step, and the `if you gained
    /// 5 or more life this turn` of an Angel-making one.
    ///
    /// The first condition here whose window is the **turn** rather than the
    /// resolution, and it reads the recorded [`GameEvent::LifeChanged`](crate::GameEvent)
    /// entries for exactly the reason [`TriggerCondition::YouGainLife`] does: the
    /// question is about the *events*, not the net. Gaining three life and losing it
    /// again leaves every total where it started and is still three life gained, so a
    /// comparison of life totals — against the turn's opening total or any other — would
    /// answer no to a card that means yes. Life lost is not a gain, and damage is never
    /// one, so neither subtracts from the amount.
    ///
    /// Only a lower bound exists, because only a lower bound is printed, and it counts
    /// the turn's gains **in total** rather than any single one: a card that gained
    /// three twice has gained five or more.
    GainedLifeThisTurn {
        /// The threshold, inclusive. `1` is the plain "if you gained life this turn".
        amount: u32,
    },
}

/// A class of permanents to **count**, relative to an effect's controller.
///
/// Deliberately a small product of three independent filters rather than a closed list
/// of named classes like [`MassAffects`]: a count is asked about an open-ended variety
/// of things ("artifacts you control", "Zombies you control"), and enumerating each as
/// its own variant would grow the vocabulary once per card. Nothing here selects
/// permanents to *modify*, so the `except_this` and creature-only assumptions the
/// selectors carry do not apply.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct PermanentCount {
    /// Whose permanents are counted. Defaults to the controller's own.
    #[serde(default)]
    pub scope: CountScope,
    /// Restrict to permanents with this printed card type. Absent counts every type.
    #[serde(default)]
    pub card_type: Option<CardType>,
    /// Restrict to permanents with this printed subtype. Absent counts every subtype.
    #[serde(default)]
    pub subtype: Option<String>,
    /// Restrict to permanents of this printed colour (CR 105.2) — the "**blue**
    /// creature" of a conditional static ability. Absent counts every colour, including
    /// colourless.
    #[serde(default)]
    pub color: Option<Color>,
    /// Restrict to permanents whose power is at least this — the "a creature with
    /// **power 4 or greater**" of an intervening if. Absent counts every power,
    /// including a permanent that has none.
    ///
    /// The one field here read through the **computed** characteristics rather than the
    /// printed face, because that is the only reading a printed card means: a creature
    /// pumped to 4 satisfies "power 4 or greater" and stops satisfying it when the pump
    /// ends (CR 613.1f). The others stay printed because the layers that would change a
    /// type or a colour are not implemented.
    ///
    /// That reading is also why this field is **rejected inside a static ability's
    /// condition** by the catalog validator
    /// ([`Violation::PowerInStaticCondition`](crate::Violation::PowerInStaticCondition)):
    /// asking for a computed power from inside the computation of a permanent's
    /// characteristics would not terminate.
    ///
    /// Only a lower bound exists, because only a lower bound is printed on a card the
    /// catalog defines; an upper one arrives with the card that needs it.
    #[serde(default)]
    pub min_power: Option<i32>,
}

/// Whose permanents a [`PermanentCount`] counts, relative to the effect's controller.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CountScope {
    /// The controller's own — "you control".
    #[default]
    YouControl,
    /// Every opponent's, and none of the controller's.
    OpponentsControl,
    /// Every permanent on the battlefield, whoever controls it.
    Any,
}

/// What a piece of restricted mana may be spent on (CR 106.6).
///
/// A closed set with one member today, which is the one the currently authorable cards
/// need. It grows by adding variants; a restriction nothing matches simply makes the
/// mana unspendable, which is the safe direction.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ManaRestriction {
    /// Only to cast a spell whose card has this subtype — "spend this mana only to
    /// cast **Dragon** spells".
    SpellsWithSubtype {
        /// The subtype the spell must have, as printed.
        subtype: String,
    },
}

/// Who picks the cards for an [`Effect::Discard`] — the discarding player, or the
/// controller of the spell or ability making them discard.
///
/// A separate axis from the [`PlayerRef`] naming *whose* hand it is, because the two
/// really do come apart: "target player discards two cards" is chosen by that player,
/// while "you choose a noncreature, nonland card from it; that player discards it" is
/// chosen by the caster. The chooser is also the only seat the hand is revealed to.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Chooser {
    /// The player the cards belong to — the ordinary discard.
    #[default]
    Owner,
    /// The controller of the spell or ability producing the effect.
    Controller,
}

/// Where a card taken by [`Effect::LookAtTop`] or [`Effect::SearchLibrary`] goes.
///
/// Deliberately small and closed: it names the zones the currently authorable cards put
/// a found card into, and grows by adding variants. A card entering the battlefield
/// here does so through the same seam a resolving permanent spell uses, so it mints a
/// fresh [`PermanentId`] and its enters-the-battlefield replacements and triggers all
/// fire normally.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FoundDestination {
    /// Its owner's hand.
    #[default]
    Hand,
    /// The battlefield, untapped, under the effect's controller.
    Battlefield,
    /// The battlefield, tapped, under the effect's controller (CR 701.19's "onto the
    /// battlefield tapped").
    BattlefieldTapped,
}

/// Which cards of a zone a mid-resolution choice may pick — the card-selection
/// counterpart of [`TargetSpec`], for a card in a hidden zone rather than an object on
/// the battlefield.
///
/// Kept separate from [`TargetSpec`] because it selects over a *pile of cards*, where
/// only printed characteristics exist: a card in a library or a hand has no computed
/// power, no controller, and no [`PermanentId`]. Deliberately small; it grows by adding
/// variants as cards need them.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CardFilter {
    /// Every card in the zone.
    #[default]
    Any,
    /// A land card.
    Land,
    /// A creature card, optionally capped at a printed power and/or narrowed to a
    /// printed subtype — the "creature card with power 2 or less" of a look-and-take,
    /// and the "Zombie creature" of a graveyard-casting permission.
    Creature {
        /// The greatest printed power a matching creature may have. Absent means any.
        #[serde(default)]
        max_power: Option<i32>,
        /// The printed subtype a matching creature must have. Absent means any.
        #[serde(default)]
        subtype: Option<String>,
    },
    /// A card that is neither a creature nor a land — the class a hand-attack names.
    NoncreatureNonland,
    /// A creature **or** land card — the class a look-at-the-top-four names as one
    /// choice rather than two (`you may reveal a creature or land card from among
    /// them`).
    CreatureOrLand,
    /// A **permanent** card (CR 110.1): one that would enter the battlefield when it
    /// resolves. The class a search that puts its find directly onto the battlefield
    /// names, because nothing else could go there.
    Permanent,
    /// A card with this printed subtype, whatever its card type — "a **Zombie** card",
    /// which is a Zombie creature, a Zombie artifact, or anything else printed with the
    /// subtype. Distinct from [`Self::Creature`]'s `subtype`, which also demands the
    /// creature type.
    Subtype {
        /// The printed subtype, as printed.
        subtype: String,
    },
    /// A card with the same printed identity as the effect's own source — "a card named
    /// *this card*". Matched on the functional card, so two copies of one printing do
    /// find each other and a differently-named card never does.
    SameNameAsSource,
    /// A card of a printed **colour** (CR 105.2) — "a **white** card", the class the
    /// planeswalker-deck look-and-take spells name.
    ///
    /// Matched against [`CardData::colors`](crate::CardData::colors), which is the
    /// card's printed colour indicator and not its mana cost: a colourless artifact
    /// matches no colour, and a gold card matches each of its own. Colour-changing
    /// effects are not modelled, so printed colour is current colour here exactly as it
    /// is in the blocking restriction that names one.
    Color {
        /// The colour a matching card must be.
        color: Color,
    },
    /// An **instant or sorcery** card — one class as a card writes it, in the same
    /// sense [`ObservedSpell::InstantOrSorcery`] is one class rather than two types.
    InstantOrSorcery,
    /// An **artifact** card.
    Artifact,
}

/// The class of permanents a **mass, non-targeting** effect ([`Effect::PumpAll`],
/// [`Effect::GrantKeywordAll`]) applies to.
///
/// Deliberately separate from [`StaticAffects`], which selects for a *continuous*
/// ability and carries an `except_this` that a one-shot spell has no "this" for — but
/// authored in the same internally tagged shape, so the two selectors read alike:
/// `{"kind":"pump_all","affects":{"scope":"creatures_you_control"},"power":2,"toughness":1}`.
/// It grows by adding variants (attacking creatures, tapped creatures, …) as cards
/// need them.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "scope", rename_all = "snake_case")]
pub enum MassAffects {
    /// Every creature the effect's controller controls at the moment it resolves.
    CreaturesYouControl {
        /// Restrict to creatures whose subtypes include this one — the `Dragons` of
        /// "Dragons you control get +1/+0 until end of turn". Absent means every
        /// creature its controller controls.
        #[serde(default)]
        subtype: Option<String>,
    },
    /// Every creature on the battlefield at the moment the effect resolves,
    /// whoever controls it — the symmetric class a sweeper names.
    EachCreature,
    /// Every creature controlled by an opponent of the effect's controller, at the
    /// moment it resolves. The mirror of [`Self::CreaturesYouControl`], and the
    /// reason both are relative to the controller rather than to a seat: one
    /// authored card must mean "you" from either side of the table.
    CreaturesYourOpponentsControl,
    /// Every creature on the battlefield that does not currently have flying, whoever
    /// controls it — the scope of an effect that clears the ground.
    ///
    /// Flying is read through the computed keywords (CR 613.1f), so a creature that was
    /// *granted* flying is outside the class exactly as a printed flyer is. The class is
    /// still evaluated once, on resolution (CR 611.2c), like every other mass effect: a
    /// creature that loses flying later in the turn does not retroactively join it.
    CreaturesWithoutFlying,
    /// Every creature currently **attacking**, whoever controls it — the class a combat
    /// pump names (`Attacking creatures get +2/+0 until end of turn.`).
    ///
    /// Read off [`Permanent::attacking`](crate::Permanent), so it is exactly the set the
    /// declare-attackers step produced, and it is locked in on resolution like every
    /// other mass class (CR 611.2c): a creature removed from combat afterwards keeps the
    /// pump, and one that was never in it never had one. The class is empty outside
    /// combat, which makes such a spell a legal but pointless main-phase cast rather
    /// than an uncastable one.
    AttackingCreatures,
}

/// **Who or what** an [`Effect::DealDamage`] deals its damage to (CR 120.3).
///
/// The same design [`PlayerRef`] states for life change: the *subject* declares
/// whether a target is chosen (CR 115.1), so one damage verb covers both "deals 2
/// damage to any target", which fills a slot at announcement and fizzles when that
/// slot empties, and "deals 2 damage to each opponent", which chooses nothing and
/// can never fizzle. The class forms lock their affected set in **on resolution**,
/// exactly as [`Effect::PumpAll`] does.
///
/// Authored in serde's **externally tagged** form, flattened into the effect, so the
/// key a card writes is the vocabulary it already uses elsewhere — `target` for a
/// target spec, `player_ref` for a class of players, `affects` for a class of
/// permanents:
///
/// ```json
/// {"kind": "deal_damage", "target": "any_target", "amount": 2}
/// {"kind": "deal_damage", "player_ref": "each_opponent", "amount": 2}
/// {"kind": "deal_damage", "affects": {"scope": "each_creature"}, "amount": 1}
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DamageSubject {
    /// One **chosen target** (CR 115.1): a creature, a player, or — for a burn
    /// spell — [`TargetSpec::AnyTarget`].
    Target(TargetSpec),
    /// A class of **players**, named by the same reference life change and mill
    /// take. A non-targeting reference (`controller`, `each_opponent`) chooses
    /// nothing; a targeting one behaves exactly as the equivalent
    /// [`Self::Target`] does, because the reference answers the targeting question
    /// in one place for every effect that takes one.
    #[serde(rename = "player_ref")]
    Players(PlayerRef),
    /// A class of **permanents**, named by the same selector mass pump takes. Never
    /// a target, and so never a fizzle.
    #[serde(rename = "affects")]
    Permanents(MassAffects),
}

impl DamageSubject {
    /// The [`TargetSpec`] this subject chooses a target for, or `None` when it names
    /// a class without targeting (CR 115.1).
    ///
    /// Exhaustive, so a new subject must declare which side of the targeting line it
    /// falls on; [`Effect::target_spec`] defers to this, which is what keeps the
    /// class form out of the slot-filling and fizzle paths without either of them
    /// naming damage specially.
    #[must_use]
    pub fn target_spec(&self) -> Option<TargetSpec> {
        match self {
            DamageSubject::Target(spec) => Some(*spec),
            DamageSubject::Players(player_ref) => player_ref.target_spec(),
            DamageSubject::Permanents(_) => None,
        }
    }
}
