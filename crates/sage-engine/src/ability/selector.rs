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
    /// The effect's controller controls **no** permanents matching `permanents` — the
    /// `when you control no permanents with phylactery counters on them` a Lich watches
    /// for.
    ///
    /// The negation rather than a zero threshold, because a threshold of zero is
    /// satisfied by an empty board and by a full one alike: "at least none" is true of
    /// every game state and says nothing. Written as its own question so a card that
    /// means "none" reads as one.
    ControlsNone {
        /// Which permanents are looked for, relative to the effect's controller.
        permanents: PermanentCount,
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
    /// The resolving object is a **spell that was cast from its controller's hand**
    /// (CR 601.2a) — the `if this spell was cast from your hand` of a card that pays out
    /// only when it was played the ordinary way.
    ///
    /// Read off what the resolution knows about itself rather than from any zone: by the
    /// time it resolves the card is on the stack, and every road that put it there ends in
    /// the same place. False for an ability, which is not cast at all.
    CastFromHand,
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
    /// The effect's **own source attacked or blocked this turn** — the `if this creature
    /// attacked or blocked this turn` of an end-step trigger that takes the creature
    /// back.
    ///
    /// The first condition here about the *source* rather than about its controller, and
    /// the reason [`condition_holds`](crate::condition) is handed one: "you" is not the
    /// subject of this sentence, one particular permanent is, and a card that asks it
    /// means the object its ability is on.
    ///
    /// Read off the declarations the turn recorded
    /// ([`GameEvent::AttackersDeclared`](crate::GameEvent) and
    /// [`GameEvent::BlockersDeclared`](crate::GameEvent)) rather than off
    /// [`Permanent::attacking`](crate::Permanent) and
    /// [`Permanent::blocking`](crate::Permanent), because by the end step both of those
    /// have been cleared: the end-of-combat turn-based action removes every creature from
    /// combat (CR 511.3), so a snapshot taken when this condition is asked can no longer
    /// tell a creature that attacked from one that stayed home. The declaration is the
    /// event, and the event is what survives it.
    ///
    /// Both halves are one question because a printed card asks them as one. The window is
    /// the **turn**, like [`Self::GainedLifeThisTurn`]'s, so two combats in one turn both
    /// count and the next turn starts clean.
    AttackedOrBlockedThisTurn,
}

/// The `X` of `where X is …` — a number an effect takes off the **game** rather than
/// off its own printed text.
///
/// Taken once, at the moment the effect applies (CR 608.2), and never re-read: the
/// resulting number is what the effect uses, so a permanent that leaves afterwards
/// changes nothing about an amount already fixed.
///
/// Deliberately closed and **not composable** — there is no arithmetic here, no halving,
/// and no way to add two sources together. Each variant is a phrase a printed card
/// writes, and a card that needs a new phrase adds a variant.
///
/// A **count of permanents** is deliberately not one of them: it keeps its own spelling
/// ([`PermanentCount`], authored as `count_of`), because it is the one source a *static*
/// ability may also read — an Aura's `+1/+1 for each Forest you control` is recalculated
/// on every read of its host's characteristics, and has to be. Nothing here could stand
/// in that position: two of the four read *events* over a window, exactly as
/// [`Condition`] does and for the same reason, and an event window has no meaning
/// outside a resolution. A **count of cards in a graveyard** keeps its own spelling too
/// ([`GraveyardCount`]), for that reason and one more: the only card that reads one is a
/// characteristic-defining ability ([`Ability::DefinedPower`](crate::Ability)),
/// re-derived on every read of a permanent's power rather than taken on a resolution.
///
/// **A chosen permanent's power is not one of these either** ([`PermanentAmount`]). Every
/// source below is a question about the *game*, answerable wherever the effect that reads
/// it stands; that one is a question about an object the same effect is about to remove,
/// so CR 608.2h makes it readable only *before* the removal — which is why it rides on
/// the effect that does the removing instead of standing here.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "source", rename_all = "snake_case")]
pub enum DerivedAmount {
    /// How much life the effect's **controller has gained this turn** (CR 118.3) — the
    /// `where X is the amount of life you gained this turn` of a drain that grows with a
    /// lifegain deck.
    ///
    /// The very number [`Condition::GainedLifeThisTurn`] compares against, read as a
    /// quantity instead of as a yes: a sum of the turn's gains, never a net and never a
    /// maximum. Gaining three and losing it again is still three gained, which is why a
    /// difference of life totals would answer wrongly and the recorded events are read
    /// instead. Life the *same resolution* gained a moment ago is included — a card that
    /// gains one life and then reads this means it.
    LifeGainedThisTurn,
    /// How many cards **this resolution milled** that match `filter` — the `for each land
    /// card put into their graveyard this way` of a mill-and-draw.
    ///
    /// The counting form of [`Condition::MilledThisWay`], over the same window and read
    /// off the same recorded [`GameEvent::CardsMilled`](crate::GameEvent) entries: a land
    /// already in that graveyard was not milled this way, and a graveyard scan could
    /// never tell the two apart.
    ///
    /// Whose library the cards came from is deliberately **not** a field. A resolution's
    /// mills belong to the resolution, and the card that reads this number is the same
    /// card that just did the milling — "their graveyard" names the player the effect
    /// above it already named, so a scope here would be a second way to say the same
    /// thing and a second way to get it wrong.
    MilledThisWay {
        /// Which milled cards are counted. Defaults to all of them.
        #[serde(default)]
        filter: CardFilter,
    },
    /// The **greatest mana value** among permanents matching `among` (CR 202.3) — the
    /// `equal to the greatest mana value among artifacts you control` of a draw spell
    /// that pays off a heavy board.
    ///
    /// Zero when nothing matches, which is what makes such a spell a legal but blank cast
    /// rather than an uncastable one. Mana value is read off the **printed** face, where
    /// CR 202.3b puts it, so a token — which has no mana cost — contributes zero and an
    /// Aura or a land contributes what its cost says.
    GreatestManaValue {
        /// Which permanents are looked at, relative to the effect's controller.
        among: PermanentCount,
    },
    /// The **X its controller announced** as the spell was cast (CR 601.2b) — the `X` of
    /// `deals X damage to any target` on a spell whose cost is `{X}{R}`.
    ///
    /// The one member of this vocabulary that reads neither the board nor the event log,
    /// because there is nothing to read: X was *chosen*, before targets and before
    /// payment, and locked the moment it was announced. It rides on the stack object
    /// from that point on ([`StackObjectKind::Spell`](crate::StackObjectKind)), so the
    /// mana that was charged, the effect that resolves, and the text a player reads are
    /// all the same number by construction rather than by three agreeing lookups.
    ///
    /// Zero for an object that announced no X at all, which is every ability and every
    /// spell whose cost has no `{X}` in it. That is the safe direction and it is also
    /// the honest one: such an effect has no X, and an effect that reads one it never
    /// had should do nothing rather than guess.
    AnnouncedX,
    /// How many permanents **this resolution has sacrificed** (CR 701.17) — the `that
    /// many` of `Sacrifice any number of lands. Search your library for up to that many
    /// land cards`.
    ///
    /// The counting form of an [`Effect::Sacrifice`](crate::Effect) that came earlier in
    /// the same resolution, and it reads the resolution's own record
    /// ([`Resolution`](crate::Resolution)) rather than the board or the log. It has to
    /// read *something* rather than count again: the lands are in a graveyard among every
    /// other land that ever went there, and only a creature's departure is recorded as an
    /// event at all (CR 700.4).
    ///
    /// Zero for a resolution that sacrificed nothing — including one whose sacrifice was
    /// answered with none, and one that never posed the question because the board held
    /// nothing of the class. Both are what `Sacrifice any number of lands` means on an
    /// empty board: a legal, blank spell.
    ///
    /// NOTE: a sacrifice posed to *several* players records only the answer the resolution
    /// resumed on. No printed card reads a count back across seats, and the one that reads
    /// it back at all names a single player twice.
    SacrificedThisWay,
    /// The **power the creature sacrificed to this object's cost had** as it left
    /// (CR 608.2h) — the `equal to the sacrificed creature's power` of a spell that throws
    /// it at something.
    ///
    /// Last-known information, read off the same [`PaidCost`](crate::PaidCost) and fixed
    /// at the same moment, for the reason that rule exists: the creature is gone before
    /// the spell resolves, so there is nothing on the battlefield left to ask. Zero when
    /// the cost sacrificed no creature, and zero for a power that was negative — damage is
    /// never negative (CR 120.1).
    SacrificedCreaturePower,
    /// **Half** of `of`, rounded up — the `half their life, rounded up` of a symmetric
    /// sorcery that takes half of everything.
    ///
    /// The one source here that is *arithmetic over* another number, and it is a variant
    /// rather than the beginning of an expression language on purpose: "half, rounded up"
    /// is a phrase printed cards write about a handful of named totals, not an operator
    /// they compose. There is no doubling, no adding two sources together, and no
    /// halving of a half.
    ///
    /// **Rounding is up and is not a field.** Every card that halves a total says which
    /// way it rounds, and the ones that round down are a different phrase; when one is
    /// authored it adds its own variant rather than a flag here, so no card can be
    /// written that rounds the direction its text does not say.
    ///
    /// The total is read of the player the effect *names*, not of its controller — the
    /// point of `each player loses half their life` is that each of them loses their own
    /// half — and it is read once, where the effect applies (CR 608.2).
    HalfRoundedUp {
        /// Which total is halved.
        of: HalvedTotal,
    },
}

/// The class of permanents a **mass destruction** puts into their owners' graveyards
/// (CR 701.7) — the `all creatures` and the `all artifacts and enchantments` of a
/// sweeper's two modes.
///
/// Its own vocabulary rather than a widening of [`MassAffects`], which every existing
/// member of is a class of *creatures* feeding a pump or a keyword grant: a
/// non-creature scope there would make "artifacts you control get +1/+1" an authorable
/// sentence that means nothing. Closed and named, for [`MassAffects`]'s reason — a
/// disjunction of two card types is not a product of independent filters and
/// [`PermanentCount`] could not say it — and it grows by adding variants.
///
/// The affected set is enumerated **on resolution** (CR 611.2c), so a permanent that
/// arrives afterwards survives.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "scope", rename_all = "snake_case")]
pub enum DestroyAffects {
    /// Every creature on the battlefield, whoever controls it.
    EachCreature,
    /// Every artifact and every enchantment on the battlefield, whoever controls it. One
    /// class rather than two, because the printed sentence is one destruction and a
    /// permanent that is both is destroyed once.
    EachArtifactOrEnchantment,
}

/// A total a [`DerivedAmount::HalfRoundedUp`] takes half of, asked about the player the
/// effect names.
///
/// A closed list of the three a printed card halves, and each one is a *snapshot*
/// question — a life total, a hand, a board — so all three are answered from the state as
/// the effect is reached rather than from recorded events.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HalvedTotal {
    /// That player's **life total** (CR 119.1) — `loses half their life`. A player
    /// already at or below zero has no positive total to halve and loses nothing.
    LifeTotal,
    /// How many cards are in that player's **hand** — `discards half the cards in their
    /// hand`. Counted as the effect is reached, so a discard earlier in the same
    /// resolution has already made the hand smaller.
    HandSize,
    /// How many **creatures that player controls** — `sacrifices half the creatures they
    /// control`. Control is read through the one CR 613 layer-2 path, so a creature
    /// someone has taken counts for whoever controls it now.
    CreaturesControlled,
}

/// A number read off **one chosen permanent** — the `its power` of `Exile target
/// colorless creature. You gain life equal to its power.`
///
/// Deliberately not a [`DerivedAmount`]: that vocabulary answers questions about the
/// *game*, which stay answerable wherever the effect that asks them stands. This one asks
/// about an object the very same effect is about to remove, and CR 608.2h says the answer
/// is what was last known of it — so it can only be read *before* the removal, which is
/// why it is a field on the effect that removes rather than a source a later effect could
/// name.
///
/// One variant today, and it grows by adding them: a card that pays off a chosen
/// permanent's toughness or mana value writes its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermanentAmount {
    /// The permanent's **power** as the effect found it — the *computed* power (CR 613),
    /// so a creature pumped or grown by counters is worth what it currently is rather
    /// than what it printed. A permanent with no power at all is worth nothing.
    Power,
}

/// A class of **cards in a graveyard** to count.
///
/// The graveyard counterpart of [`PermanentCount`], and separate from it for the reason
/// [`CardFilter`] is separate from [`TargetSpec`]: a card in a graveyard has printed
/// characteristics and nothing else — no controller, no [`PermanentId`], no computed
/// power — so the two selectors have almost no fields in common.
///
/// It is read where the question is asked and counted fresh every time, because the one
/// thing that asks is a characteristic-defining ability
/// ([`Ability::DefinedPower`](crate::Ability)).
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct GraveyardCount {
    /// Whose graveyards are counted, relative to the reading object's controller.
    /// Defaults to their own — "in your graveyard".
    #[serde(default)]
    pub scope: GraveyardScope,
    /// Which cards of those graveyards are counted. Defaults to all of them.
    #[serde(default)]
    pub filter: CardFilter,
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
    /// Restrict to permanents with at least one counter of this kind on them — the
    /// "permanents with **phylactery counters** on them" a Lich's life depends on.
    ///
    /// Read off the permanent's own counters, which is where a counter is: it is not a
    /// characteristic, nothing in the layer system produces one, and so unlike
    /// [`min_power`](Self::min_power) this can be asked from anywhere, static conditions
    /// included.
    #[serde(default)]
    pub with_counter: Option<CounterKind>,
    /// Count only permanents that are **not tokens** (CR 111) — the "number of
    /// **nontoken** creatures you control" a card counts before making tokens of its own.
    ///
    /// A flag rather than a three-way "token / nontoken / either", because only one
    /// direction is printed: a card that counted *tokens* would add the other value here
    /// and nothing about this field would have to move. Read off what the permanent is,
    /// which the state already records ([`Printed::is_token`](crate::Printed::is_token)) —
    /// never inferred from a missing card identity.
    ///
    /// It matters most on exactly the cards that carry it: a token-making effect that
    /// counted its own tokens would grow every time it resolved.
    #[serde(default)]
    pub nontoken: bool,
    /// Count how many **different names** the matching permanents have, rather than how
    /// many permanents there are — the "four or more Demons **with different names**" of
    /// a card that wins on a board it cannot build out of one card played four times.
    ///
    /// A property of the *tally*, not of the filter: every field above still says which
    /// permanents are looked at, and this says what is counted once they are. Four
    /// Demons of one name are four permanents and one name.
    ///
    /// A permanent's name is the one its **current face** prints (CR 712.4b — a
    /// transformed card is named by the face that is up), and a token's is its own
    /// (CR 111.4). Read as the printed string rather than as a card identity, because
    /// that is the question the card asks: two different printings of one card share a
    /// name, and a token that copies a card takes its name.
    ///
    /// Deliberately does nothing to
    /// [`DerivedAmount::GreatestManaValue`](super::DerivedAmount), the one other reader
    /// of this selector — and cannot, because dropping duplicates never changes a
    /// maximum. It is therefore not authorable nonsense there, merely inert.
    #[serde(default)]
    pub distinct_names: bool,
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

/// How the cards a [`Effect::LookAtTop`] did *not* take reach the bottom of the library.
///
/// The two wordings printed on these cards, and they are genuinely different rules: *put
/// the rest on the bottom of your library in a random order* is the game deciding, and
/// *in any order* is the player deciding. The distinction is worth a field rather than a
/// blanket approximation because it is the difference between three cards whose future
/// order is unknown and three cards whose future order the looker just set.
///
/// [`Random`](Self::Random) is the default, so a card that says nothing keeps the
/// conservative reading — it tells the player strictly less than the printed card does.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BottomOrder {
    /// *…in a random order.* Drawn from the seeded RNG
    /// ([`GameState::rng_seed`](crate::GameState::rng_seed)), so a replay of the same
    /// seed produces the same bottom order and the looker learns nothing about their
    /// future draws.
    #[default]
    Random,
    /// *…in any order.* The looker is asked, through the same mid-resolution choice
    /// queue the taking itself went through (ADR 0013), and the answer is the order the
    /// cards are put on the bottom in. **It consumes no randomness at all** — a player's
    /// answer is already recorded in the action log, and drawing from the RNG for a
    /// decision the player made would desynchronise every later shuffle on replay.
    Chosen,
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
    /// A card that is either a printed **colour** or an **artifact** — Tezzeret's
    /// Gatebreaker's `a blue or artifact card`.
    ///
    /// A named disjunction rather than a general "any of these filters", for
    /// [`Self::CreatureOrLand`]'s reason: the card offers it as **one** choice, not as two
    /// questions, and the vocabulary grows by naming the classes cards actually print. The
    /// colour half is matched exactly as [`Self::Color`] matches it.
    ColorOrArtifact {
        /// The colour half of the class.
        color: Color,
    },
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
        /// Restrict to creatures whose power is at least this — the "each creature you
        /// control **with power 4 or greater**" of an attack trigger. Absent means
        /// every power, including a creature that has none.
        ///
        /// Read through the **computed** characteristics (CR 613.1f), like the
        /// identically-named field on [`PermanentCount`] and for the same reason: that
        /// is the only reading a printed card means, so a creature pumped to 4 is in the
        /// class and one shrunk out of it is not. Asking for a computed power here is
        /// safe where it is not in a static ability's condition — a mass effect is
        /// enumerated **during a resolution** (CR 611.2c), from outside the layer
        /// system, so there is no computation to recurse into.
        #[serde(default)]
        min_power: Option<i32>,
        /// Restrict to creatures whose power is **strictly less than the source's** — the
        /// "creatures you control with power less than Lena's power" of a sacrifice that
        /// protects the small.
        ///
        /// A bound relative to another permanent rather than to a printed number, which is
        /// why it is its own flag beside [`min_power`](Self::CreaturesYouControl::min_power)
        /// rather than a value: the number it compares against is not knowable when the card
        /// is authored, and it changes with the source.
        ///
        /// Both sides are read through the **computed** characteristics at the moment of
        /// resolution (CR 613.1f / CR 611.2c), so a source pumped before the ability resolves
        /// protects more, and a creature pumped past it drops out. Safe for the same reason
        /// `min_power` is: a mass effect is enumerated from inside a resolution, outside the
        /// layer system, so there is no computation to recurse into.
        ///
        /// A source that has **left** — sacrificed to its own cost, which is exactly what
        /// Lena does — takes its power with it, and the class is then empty rather than
        /// everything: "less than Lena's power" with no Lena is not a bound that lets every
        /// creature in. The caller reads the source's power *before* paying the cost and
        /// passes it in.
        #[serde(default)]
        below_source_power: bool,
    },
    /// Every creature on the battlefield at the moment the effect resolves,
    /// whoever controls it — the symmetric class a sweeper names.
    EachCreature,
    /// Every creature controlled by an opponent of the effect's controller, at the
    /// moment it resolves. The mirror of [`Self::CreaturesYouControl`], and the
    /// reason both are relative to the controller rather than to a seat: one
    /// authored card must mean "you" from either side of the table.
    CreaturesYourOpponentsControl,
    /// Every **creature and planeswalker** an opponent of the effect's controller
    /// controls — the wider class a sweeper that also burns walkers names.
    ///
    /// A class rather than two, because the card prints it as one breath: `deals 4 damage
    /// to each opponent and each creature and planeswalker they control`. Damage to a
    /// planeswalker removes loyalty (CR 120.3c) at the same seam damage to a creature is
    /// marked, so nothing about the verb has to know which it hit.
    CreaturesAndPlaneswalkersYourOpponentsControl,
    /// Every creature controlled by the player this resolution's most recent targeted
    /// effect named — the `each creature **that player** controls` of a spell that hits a
    /// player and their board in one sentence.
    ///
    /// The class counterpart of [`PlayerRef::ThatPlayer`], reading the same fact for the
    /// same reason: the choice belongs to the sentence before it.
    CreaturesThatPlayerControls,
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
