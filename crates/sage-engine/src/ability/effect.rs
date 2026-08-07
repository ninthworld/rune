//! The [`Effect`] vocabulary: every single thing an ability or a spell can do, and the
//! questions asked about one.

use super::*;

mod targets;

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
    /// Add `amount` mana whose **colors the player chooses** as the effect resolves —
    /// `Add two mana in any combination of colors.`, and, with
    /// [`same_color`](Self::AddManaAnyColor::same_color) set, `Add two mana of any one
    /// color.`
    ///
    /// The choice is real, and how many times it is asked is the whole of the difference
    /// between the two printed phrases: a combination asks once per point, so two mana
    /// may be one each of two colors, while "of any one color" asks once and pays out
    /// the whole amount in the answer. The questions ride the ordinary mid-resolution
    /// choice queue ([`crate::ChoiceQuestion::Color`]), which is why an effect that
    /// looks like a variant of [`Effect::AddMana`] is a separate verb — the amount is
    /// fixed, but the *colors* are not authored at all.
    ///
    /// The optional `restriction` rides on every point produced, exactly as
    /// [`Effect::AddRestrictedMana`]'s does (CR 106.6), so `Add two mana in any
    /// combination of colors. Spend this mana only to cast Dragon spells.` is one
    /// effect rather than a colored one repeated five ways.
    AddManaAnyColor {
        /// How many mana are produced in total.
        amount: u8,
        /// Whether all of it must be **one** color — one question answered once, rather
        /// than one question per point. Defaults to `false`, the combination form every
        /// card authored before a land Aura needed the other one writes.
        #[serde(default)]
        same_color: bool,
        /// What the produced mana may be spent on (CR 106.6). Absent means
        /// unrestricted.
        #[serde(default)]
        restriction: Option<ManaRestriction>,
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
    /// Tap **every creature a named player controls**, and optionally stop those same
    /// creatures untapping in that player's next untap step (CR 502.4) — `Tap all
    /// creatures target player controls. Those creatures don't untap during that
    /// player's next untap step.`
    ///
    /// The subject is a [`PlayerRef`] exactly as [`Effect::Mill`]'s is, and decides on
    /// its own whether a target is chosen: `target_player` fills a slot and can fizzle,
    /// `each_opponent` fills none and cannot. It is deliberately **not** a
    /// [`MassAffects`] class: every one of those is read relative to the effect's
    /// controller and none of them targets, so "creatures *that player* controls" is
    /// unsayable in that vocabulary and sayable in this one without inventing anything.
    ///
    /// The skip rides on this effect rather than beside it as a second effect for the
    /// reason a pump carries its keywords: one effect declares one target group, so two
    /// effects would advertise two slots and let a player tap one seat's creatures while
    /// stopping another seat's untapping.
    ///
    /// The affected set is enumerated **on resolution** (CR 611.2c), so a creature that
    /// arrives afterwards is neither tapped nor flagged.
    TapAll {
        /// Whose creatures are tapped. A targeting reference fills one slot.
        player_ref: PlayerRef,
        /// Whether the tapped creatures also skip their controller's next untap step.
        /// `false` is a plain mass tap.
        #[serde(default)]
        skip_next_untap: bool,
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
    /// Deal `amount` damage to what [`DamageSubject`] names (CR 120.3).
    ///
    /// The subject decides on its own whether a target is chosen: a
    /// [`DamageSubject::Target`] fills a slot at announcement (CR 601.2c) and is
    /// re-checked on resolution (CR 608.2b), while a class of players or of
    /// permanents chooses nothing and can never fizzle. Either way this is
    /// *damage*, not life loss: damage to a creature is *marked* on it (CR 120.3d)
    /// for the lethal-damage state-based action to read (CR 704.5g), and damage to
    /// a player is *lost life* (CR 120.3a), feeding the zero-life state-based
    /// action (CR 704.5a). Damage prevention/replacement and deathtouch are not
    /// modeled.
    DealDamage {
        /// Who or what takes the damage — one chosen target, or a class.
        #[serde(flatten)]
        subject: DamageSubject,
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
        /// How many of them may be targeted — the `up to two target creatures` of a
        /// planeswalker's ultimate. Defaults to exactly one, which is what every other
        /// card authoring this effect says.
        ///
        /// The fourth effect to take an arity, joining [`Effect::PutCounters`],
        /// [`Effect::Restrict`] and [`Effect::ReturnCardToHand`]: one field read in one
        /// arm of [`Effect::target_groups`], with each destruction applied to each target
        /// still legal on resolution (CR 608.2c).
        #[serde(default)]
        targets: TargetCount,
    },
    /// Destroy **every permanent in a named class** (CR 701.7) — `Destroy all
    /// creatures.`, `Destroy all artifacts and enchantments.` — the mass, non-targeting
    /// counterpart of [`Effect::Destroy`].
    ///
    /// Chooses no target, so it can never fizzle and never asks a question; the affected
    /// set is enumerated **on resolution** (CR 611.2c) and every member goes to its
    /// owner's graveyard through the same battlefield-departure seam a single destroy
    /// uses, so a token ceases to exist (CR 111.7) and a death trigger sees each one.
    ///
    /// Indestructible and regeneration are unmodeled, so nothing survives this.
    DestroyAll {
        /// The class of permanents destroyed.
        affects: DestroyAffects,
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
        /// The effect's controller **gains life equal to** this, read off the exiled
        /// permanent — `Exile target colorless creature. You gain life equal to its
        /// power.` Absent for the ordinary exile, which is nearly every one.
        ///
        /// A field on the exile rather than a life-gain effect standing after it, and
        /// CR 608.2h is the whole reason. "Its power" is a question about an object this
        /// effect has just removed from the battlefield; a second effect asking it would
        /// be asking about something that is no longer there, so the number is read
        /// *here*, before the permanent moves, and the life is gained in the same breath.
        /// The rider is therefore never a separate amount the IR could get out of order.
        #[serde(default)]
        gain_life: Option<PermanentAmount>,
    },
    /// Two chosen creatures and the damage their power deals (CR 701.12): the first
    /// deals damage equal to its power to the second, and — when the card prints the
    /// word *fights* — the second deals damage equal to its power back.
    ///
    /// **The effect that names two differently-specified targets.** Every other
    /// targeting effect in the IR declares one [`TargetSpec`] and takes its slots from
    /// that one class; this one declares a spec *per slot*, which is what lets a card
    /// say "target creature you control" and "target creature you don't control" in one
    /// sentence. Splitting it into two effects would not do: two effects are aimed
    /// independently, and the whole content of this one is that the two creatures it
    /// names are related to each other.
    ///
    /// The amount is not an [`Amount`](crate::Amount)-style expression and does not need
    /// one. CR 701.12a *defines* fighting as each creature dealing damage equal to its
    /// power, so the power read is part of the verb rather than a number the card
    /// supplies — which is why this effect has no amount field at all. Both powers are
    /// read before either damage is dealt, so the damage is simultaneous (CR 701.12a)
    /// and a creature that dies still dealt its full power.
    ///
    /// # When one of them has gone (CR 701.12c)
    ///
    /// If either creature is an illegal target as the effect is reached — it left the
    /// battlefield, stopped being a creature, gained hexproof — **neither** deals nor is
    /// dealt damage. That is stricter than the CR 608.2c default of doing as much as
    /// possible, and it is the rule for every multi-slot effect here: the slots are not
    /// interchangeable, so half of one is not a smaller version of it. When *every*
    /// target is illegal the object never resolves at all (CR 608.2b), which the resolve
    /// path settles before this effect is reached.
    ///
    /// The damage has a *source* — a permanent, unlike every other damage this IR deals
    /// — so deathtouch (CR 702.2b) and lifelink (CR 702.15e) on either creature apply,
    /// through the same seams combat damage uses.
    Fight {
        /// The first slot: the creature that deals damage equal to its power. Almost
        /// always `any_creature_you_control`.
        dealer: TargetSpec,
        /// The second slot: the creature that damage is dealt to. Almost always
        /// `any_creature_an_opponent_controls`, the class a card prints as "target
        /// creature you don't control".
        dealt_to: TargetSpec,
        /// Whether the second creature deals damage equal to *its* power back to the
        /// first — the printed word **fights** (CR 701.12a). `false`, the default, is
        /// the one-sided form a card prints as "deals damage equal to its power to".
        #[serde(default)]
        mutual: bool,
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
    ///
    /// The one effect in the IR that may name **more than one** target: `put a +1/+1
    /// counter on each of up to two target creatures` is one effect with a two-slot
    /// group ([`targets`](Self::PutCounters::targets)), applied once per target that is
    /// still legal. Omitting the field leaves it at one target, which is what every
    /// other card that uses this effect says.
    PutCounters {
        /// What each of this effect's slots is allowed to target (a permanent that can
        /// bear counters).
        target: TargetSpec,
        /// How many targets are chosen. Defaults to exactly one.
        #[serde(default)]
        targets: TargetCount,
        /// The kind of counter to place. Named `counter` on the wire because the
        /// effect enum already reserves the `kind` tag for its own discriminant.
        counter: CounterKind,
        /// How many counters of that kind to place on each target.
        ///
        /// Ignored when [`count_amount`](Self::PutCounters::count_amount) is present,
        /// which is where the number comes from then.
        #[serde(default)]
        count: u32,
        /// Where the number comes from when the card does not print one — the `X +1/+1
        /// counters, where X is your life total` of a planeswalker's ultimate.
        ///
        /// A field rather than a second verb, for the reason
        /// [`Effect::SearchLibrary`]'s `take_amount` is one: a twin variant would
        /// duplicate the target, the arity and the counter kind, and the number is the
        /// *same* number this effect already carries. Taken **once**, as the effect
        /// resolves (CR 608.2), and applied identically to every target still legal — a
        /// card that names two creatures gives each of them the same X.
        #[serde(default)]
        count_amount: Option<DerivedAmount>,
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
    ///
    /// The optional `keywords` are granted to **that same creature**, which is the
    /// whole reason they live here rather than in a second [`Effect::GrantKeyword`]
    /// beside this one: one effect declares one target group, so a combat trick
    /// printed as `Target creature gets +2/+2 **and** gains flying` must be one
    /// effect or the engine would offer two independent slots and let a player pump
    /// one creature while another gained flying. Same shape as an Aura's grant, which
    /// carries P/T and keywords together for the same reason (CR 613.1f).
    Pump {
        /// What this effect is allowed to target (a creature).
        target: TargetSpec,
        /// The signed amount added to the target's power until end of turn.
        power: i32,
        /// The signed amount added to the target's toughness until end of turn.
        toughness: i32,
        /// Keyword abilities granted to the same target until end of turn, applied at
        /// CR 613 layer 6 exactly as [`Effect::GrantKeyword`] applies one. Empty for
        /// the ordinary pump that only changes numbers.
        #[serde(default)]
        keywords: Vec<Keyword>,
        /// **Written-out** abilities granted to the same target until end of turn — the
        /// `and gains "When this creature dies, return it to the battlefield tapped under
        /// its owner's control"` of a black combat trick — applied at CR 613 layer 6 as a
        /// [`Modification::GrantAbility`](crate::Modification::GrantAbility) keyed to that
        /// one permanent. Empty for every pump that grants only words from the keyword
        /// list.
        ///
        /// Beside [`keywords`](Self::Pump::keywords) rather than in a verb of its own for
        /// the reason the keywords are beside the numbers: one effect declares one target
        /// group, and a card that says `gets +2/+0 **and** gains "…"` names one creature.
        /// Unlike a keyword grant it is not idempotent — two castings give two abilities,
        /// because two grants are two abilities (CR 613.1f adds, it does not merge).
        ///
        /// A granted **dies** trigger is the shape the catalog needs it for, and it is
        /// the one grant that outlives the grant: the trigger fires on the way out
        /// (CR 603.6c), read from the state the permanent was still in and still granted
        /// in, and what its effects then do to the card in the graveyard is the ability's
        /// business rather than the creature's.
        #[serde(default)]
        abilities: Vec<Ability>,
        /// Combat restrictions imposed on the same target until end of turn, applied at
        /// CR 613 layer 6 exactly as [`Effect::Restrict`] imposes one — including the
        /// one *requirement* in that vocabulary, `all creatures able to block it do so`.
        /// Empty for the ordinary pump.
        ///
        /// Beside [`keywords`](Self::Pump::keywords) rather than in a second effect for
        /// exactly the reason the keywords are: one effect declares one target group, so
        /// a combat trick printed as *target creature gets +3/+3 **and** every creature
        /// able to block it does so* has to be one effect, or the engine would offer two
        /// independent slots and let a player pump one creature while another was bound.
        #[serde(default)]
        restrictions: Vec<CombatRestriction>,
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
    /// **Gain control** of the single creature this effect targets until end of turn
    /// (CR 613 layer 2) — the theft verb, optionally untapping it and granting it
    /// keywords in the same breath.
    ///
    /// On resolution it adds a timestamped [`Modification::GainControl`](crate::Modification::GainControl)
    /// keyed to that one permanent with an `UntilEndOfTurn` duration, which the cleanup
    /// step removes (CR 514.2) — at which point control simply reverts, because nothing
    /// was ever written onto the permanent (ADR 0005). Layer 2 is applied before every
    /// other layer the engine models, so the change is visible to *everything* at once:
    /// who may attack with it, who may activate it, whose "creatures you control" counts
    /// it, and who its combat damage comes from.
    ///
    /// A control change **re-triggers summoning sickness** (CR 302.6) — the creature has
    /// not been under its new controller's control since their turn began — which is
    /// exactly why the printed cards that do this also grant haste.
    ///
    /// The untap and the keywords ride on this effect rather than beside it as separate
    /// ones, for the reason [`Effect::Pump`]'s keywords do: one effect declares one
    /// target group, so three effects would advertise three slots and let a player steal
    /// one creature, untap a second, and haste a third. That is also why there is no
    /// standalone untap verb — the only card in the catalog that untaps is untapping the
    /// creature it just took.
    ///
    /// The permanent is **not** removed from combat here (CR 506.4). It cannot be in
    /// combat: a control change is only ever gained at sorcery speed today, and no combat
    /// declaration survives a main phase.
    GainControl {
        /// What this effect is allowed to target (a creature).
        target: TargetSpec,
        /// Whether the stolen permanent is also untapped (CR 701.20). `false` leaves it
        /// exactly as tapped as it was.
        #[serde(default)]
        untap: bool,
        /// Keyword abilities granted to the same permanent until end of turn, applied at
        /// CR 613 layer 6 exactly as [`Effect::Pump`]'s are. In practice haste, without
        /// which a freshly stolen creature could not attack this turn.
        #[serde(default)]
        keywords: Vec<Keyword>,
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
    ///
    /// The third effect that may name **more than one** target, beside
    /// [`Effect::PutCounters`] and [`Effect::ReturnCardToHand`]: `up to two target
    /// creatures can't be blocked this turn` is one effect with a two-slot group,
    /// imposed once per target still legal on resolution (CR 608.2c). Omitting the
    /// field leaves it at one target, which is what every other card using this effect
    /// says.
    Restrict {
        /// What each of this effect's slots is allowed to target (a creature).
        target: TargetSpec,
        /// How many targets are chosen. Defaults to exactly one.
        #[serde(default)]
        targets: TargetCount,
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
    /// Change what abilities **this ability's own source** has until end of turn at
    /// CR 613 **layer 6** — `loses defender and gains flying until end of turn`, or
    /// `loses all abilities until end of turn`.
    ///
    /// One effect for the whole clause, the way [`Effect::Pump`] carries its keywords
    /// and [`Effect::GainControl`] carries its untap: a card that both subtracts and
    /// adds prints one sentence about one permanent, and splitting it into two effects
    /// would make the two halves separately timestamped and separately skippable.
    /// Within the clause the subtraction is applied before the addition — the order the
    /// card prints, and the order that makes `loses defender and gains defender`
    /// (nobody's card, but a shape the IR can say) mean what it reads as.
    ///
    /// Between clauses it is the **timestamp** that decides (CR 613.1f): a later grant
    /// puts back what an earlier removal took, and a later removal takes back what an
    /// earlier grant gave. Nothing here is retroactive and nothing is permanent — the
    /// whole effect is `UntilEndOfTurn` and the cleanup step removes it (CR 514.2), at
    /// which point the printed abilities are simply there again, because they were
    /// never taken off the card (ADR 0005).
    ///
    /// Like [`Effect::PumpSelf`] the subject is implicit: the source is not a *target*
    /// (CR 115.1), so this chooses nothing, fills no slot, and can never fizzle. A
    /// source that has left the battlefield is not there to change, and the effect does
    /// nothing.
    AlterAbilitiesSelf {
        /// Whether the source loses **all** abilities (CR 613.1f) — every keyword,
        /// every combat restriction, and every printed static, triggered, and activated
        /// ability. Applied before [`Self::AlterAbilitiesSelf::lose`] and before
        /// [`Self::AlterAbilitiesSelf::gain`], so a clause that says "loses all
        /// abilities and gains hexproof" ends with exactly hexproof.
        #[serde(default)]
        lose_all: bool,
        /// Keyword abilities the source loses, whether it printed them or was granted
        /// them earlier (CR 613.1f makes no distinction by then). Losing one it does
        /// not have does nothing.
        #[serde(default)]
        lose: Vec<Keyword>,
        /// Keyword abilities the source gains, applied after the losses so the two
        /// halves of one printed sentence do not fight.
        #[serde(default)]
        gain: Vec<Keyword>,
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
        #[serde(default)]
        count: u32,
        /// Take the count from **the number the trigger event measured** rather than from
        /// [`count`](Self::PutCountersOnSelf::count) — Hungering Hydra's `put *that many*
        /// +1/+1 counters on it`, where "that many" is the damage it was just dealt.
        ///
        /// The number is written into the effect **when the trigger is collected**, where
        /// the event that fixed it is still in hand (CR 603.3); by resolution this is an
        /// ordinary count and nothing reads this flag. That is why the substitution is not
        /// a lookup at resolution time: the damage event is long gone by then, and the
        /// ability that says "that many" means the amount it saw, not whatever the board
        /// looks like when it resolves.
        #[serde(default)]
        that_many: bool,
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
    /// The referenced player **discards** `count` cards (CR 701.8) — the first effect
    /// in the IR that stops mid-resolution and asks a player something
    /// ([`crate::PendingChoice`]).
    ///
    /// The subject is a [`PlayerRef`] exactly as [`Effect::Mill`]'s is, so
    /// `target_player` fills a target slot and `each_opponent` does not. Who *chooses*
    /// the cards is a separate question the reference cannot answer, which is why
    /// [`chosen_by`](Self::Discard::chosen_by) exists: an ordinary discard is chosen by
    /// the discarding player, while a coercive one ("choose a noncreature, nonland card
    /// from it") is chosen by the spell's controller — and only the chooser is shown
    /// the hand.
    ///
    /// Discarding from a hand with no legal card resolves without stalling: the choice
    /// is never posed (see [`crate::pending_player_choice`]), and a player asked for
    /// more cards than they hold discards what they have.
    Discard {
        /// Whose hand the cards leave.
        player_ref: PlayerRef,
        /// How many cards are discarded, at most.
        count: u8,
        /// Who picks which cards. Defaults to the discarding player themselves.
        #[serde(default)]
        chosen_by: Chooser,
        /// Which cards of that hand may be picked. Defaults to any of them.
        #[serde(default)]
        filter: CardFilter,
    },
    /// **Scry** `count` (CR 701.17): the controller looks at the top `count` cards of
    /// their library and puts any number of them on the bottom, the rest staying on
    /// top.
    ///
    /// No card leaves the library and nothing is revealed to anyone else, so this
    /// chooses no target and can never fizzle. The chosen cards go to the bottom in the
    /// order they were chosen; reordering the cards *kept on top* is not modeled.
    Scry {
        /// How many cards from the top are looked at.
        count: u8,
    },
    /// Look at the top `count` cards of the controller's library, put up to `take` of
    /// them matching `filter` into `destination`, and put the rest on the bottom — the
    /// "look at the top N and take one" verb.
    ///
    /// `take` and `take_min` are the two ends of one range, because the cards print
    /// both: *you may reveal a creature card from among them and put it into your hand*
    /// leaves the take optional, and *put one of them into your hand* does not. Whichever
    /// it is, the bounds are **clamped to what is actually there**
    /// ([`choice_bounds`](crate::choice_bounds)), so a mandatory take over a library that
    /// cannot supply one resolves with nothing taken rather than stalling — the
    /// never-stall rule owns the floor, not the card.
    ///
    /// `bottom_order` is the second half of the card, and it is a rule rather than a
    /// flourish: *in a random order* is the game deciding and *in any order* is the
    /// looker deciding, so the second poses a **second question** over the remainder
    /// through the same choice queue the taking used. A remainder of nothing or of one
    /// card is not a decision and is never asked (ADR 0013 §5).
    LookAtTop {
        /// How many cards from the top are looked at.
        count: u8,
        /// The **most** of them that may be taken.
        take: u8,
        /// The **fewest** of them a legal answer may take, before clamping. Defaults to
        /// `0` — every look printed with "you may" — so a card that says nothing keeps
        /// the take optional.
        #[serde(default)]
        take_min: u8,
        /// Which of the looked-at cards may be taken. Defaults to any of them.
        #[serde(default)]
        filter: CardFilter,
        /// Where a taken card goes. Defaults to its owner's hand.
        #[serde(default)]
        destination: FoundDestination,
        /// How the cards not taken reach the bottom. Defaults to
        /// [`BottomOrder::Random`], the conservative reading.
        #[serde(default)]
        bottom_order: BottomOrder,
    },
    /// **Search** the controller's library for up to `take` cards matching `filter`,
    /// put them into `destination`, then shuffle (CR 701.19).
    ///
    /// The searching player sees their own library and no other seat does — the
    /// hidden-information seam this effect introduces. Failing to find is always legal
    /// (CR 701.19c), so a search of a library with no match resolves, shuffles, and
    /// moves on; the shuffle happens either way and draws from the seeded RNG, so the
    /// post-search order replays identically.
    SearchLibrary {
        /// How many matching cards may be found.
        ///
        /// Ignored when [`take_amount`](Self::SearchLibrary::take_amount) or
        /// [`any_number`](Self::SearchLibrary::any_number) is present, either of which is
        /// where the number comes from then.
        take: u8,
        /// Where the number of cards comes from, when the card does not print one — the
        /// `up to that many` of `Sacrifice any number of lands. Search your library for
        /// up to that many land cards`.
        ///
        /// A field rather than a `search_library_by_amount` twin, for the reason
        /// [`Effect::CreateToken`]'s `count_of` is one: a second variant would duplicate
        /// the filter and the destination, and the number is the *same* number this effect
        /// already carries. The field says where it comes from; it does not add a verb.
        ///
        /// Taken **once**, as the effect is reached (CR 608.2), and it is a *ceiling*: the
        /// search that follows is the ordinary one, so a player may always find fewer or
        /// fail to find entirely (CR 701.19c), and an amount of zero is a search that
        /// shuffles and finds nothing rather than a stall.
        #[serde(default)]
        take_amount: Option<DerivedAmount>,
        /// Which cards of the library may be found. Defaults to any of them.
        #[serde(default)]
        filter: CardFilter,
        /// Where a found card goes. Defaults to its owner's hand.
        #[serde(default)]
        destination: FoundDestination,
        /// **Any number** of them — the `search your library for any number of Dragon
        /// creature cards` of a planeswalker's ultimate, where the card prints no ceiling
        /// at all.
        ///
        /// The open form of the same question rather than a second verb, exactly as an
        /// absent amount is the open form of [`Effect::Sacrifice`]: the ceiling becomes
        /// the whole matching set, which the choice bounds clamp to what the library
        /// actually holds. The floor is unchanged at none, because failing to find is
        /// always legal (CR 701.19c) — so "any number" already includes zero and needs no
        /// clause saying so.
        #[serde(default)]
        any_number: bool,
    },
    /// **Create** `count` tokens with the characteristics `token` describes (CR 111.1):
    /// `create a 1/1 red Goblin creature token`, `create two 1/1 white Soldier creature
    /// tokens`.
    ///
    /// The token is not a card and never was one — the effect *is* its printed face
    /// (CR 111.3), authored inline here as a [`TokenData`] with no `functional_id`, so
    /// a token can neither enter the compatibility report nor be named by a decklist.
    /// It enters the battlefield through the same seam a resolving permanent spell
    /// uses ([`GameState::create_token`](crate::GameState::create_token)), so it mints
    /// a fresh [`PermanentId`], is summoning-sick like anything else that just arrived,
    /// and is picked up by the diff-based trigger collector: an "enters the
    /// battlefield" watcher sees it exactly as it sees a creature spell resolving.
    ///
    /// The subject is a [`PlayerRef`] naming **who creates them**, exactly as
    /// [`Effect::Mill`]'s names whose library is milled — so `controller` (the
    /// default, and what every card in the first batch says) chooses no target, while
    /// `target_player` fills a slot and is re-checked on resolution (CR 608.2b). The
    /// creator is not always the controller of the ability, which is why this is a
    /// reference rather than an assumption.
    CreateToken {
        /// The characteristics of each token created (CR 111.3).
        token: TokenData,
        /// How many tokens are created. Defaults to one.
        ///
        /// With [`count_of`](Self::CreateToken::count_of) present this is how many are
        /// created **per counted permanent** rather than the total, exactly as
        /// [`Effect::GainLifeByCount`]'s `amount_per` is the life per counted permanent.
        #[serde(default = "one")]
        count: u8,
        /// Which permanents [`count`](Self::CreateToken::count) is multiplied by, if any
        /// — the `for each nontoken creature you control` of a token-making
        /// enters-the-battlefield trigger. Absent is the ordinary fixed count.
        ///
        /// A field rather than a `create_token_by_count` twin, which is where this
        /// departs from [`Effect::PumpByCount`] and its two siblings: a second variant
        /// would duplicate the four other fields here — the token's whole face, its
        /// creator, tapped, attacking — and the count is the *same* number this effect
        /// already carries. The field says where that number comes from; it does not add
        /// a second verb.
        ///
        /// X is taken **once, on resolution** (CR 608.2), from the board as it stands
        /// then: a creature that arrived while the ability was on the stack is counted
        /// and one that has died is not. Afterwards the tokens simply exist — nothing
        /// later in the turn adds one or takes one back.
        #[serde(default)]
        count_of: Option<PermanentCount>,
        /// Who creates them, and therefore who controls them. Defaults to the
        /// effect's controller ("you create …").
        #[serde(default = "PlayerRef::controller")]
        player_ref: PlayerRef,
        /// Whether each token enters **tapped** (CR 111.1 — the creating effect may
        /// say so). Defaults to `false`, an ordinary untapped entry.
        #[serde(default)]
        tapped: bool,
        /// Whether each token enters **attacking** (CR 506.3c) — `create two 1/1
        /// white Cat creature tokens that are tapped and attacking`. Defaults to
        /// `false`, and it is a sibling of [`tapped`](Self::CreateToken::tapped)
        /// rather than a mode of it: the same effect may say one, both, or neither.
        ///
        /// **What it attacks is not authored, because a card never states it.** The
        /// token joins the declaration already in progress: it attacks the player or
        /// planeswalker the effect's own source is attacking, which is what makes
        /// "that are attacking" mean the same thing on every card that prints it.
        /// Outside combat — and after the source has left it — there is no attack to
        /// join, so the tokens are created and simply are not attacking; the effect
        /// never invents a defender.
        ///
        /// It was **never declared** as an attacker (CR 506.3c), so it is not tapped
        /// by attacking, it is not restricted by summoning sickness, and no
        /// "whenever … attacks" ability triggers for it — the last of which is a rule
        /// about the *declaration*, enforced where triggers are collected rather than
        /// here.
        #[serde(default)]
        attacking: bool,
    },
    /// **You may** apply `effects`, and — when `cost` is present — only if you pay it:
    /// `you may draw a card`, `you may pay {1}. If you do, draw a card`, and `you may
    /// sacrifice another creature. If you do, this creature gets +2/+2 until end of
    /// turn`.
    ///
    /// The first effect in the IR whose mid-resolution question is a *decision* rather
    /// than a selection (CR 608.2). It suspends resolution and asks the ability's
    /// **controller** — never the priority holder, and never a player the effect merely
    /// names — through the same [`PendingChoice`](crate::PendingChoice) queue a discard
    /// or a search goes through.
    ///
    /// Declining is not a fizzle: it skips these effects and nothing else, so an
    /// optional effect sitting between two mandatory ones leaves both of them intact.
    /// A cost the controller could not pay — no mana any tapping could make, no
    /// permanent of the named class, not enough cards in hand — is never posed at all,
    /// it is declined outright, and either way the log records that the question was
    /// asked and answered, so a declined effect never reads as one that was silently
    /// dropped.
    ///
    /// **The target is chosen up front, and the yes-or-no comes on resolution.** A
    /// `may` over a single targeting effect *propagates* that effect's group
    /// ([`Effect::target_group`]), so "you may destroy target artifact" declares its
    /// slot at announcement (CR 601.2c) like any other targeting ability and the
    /// chosen target rides the [`ConfirmRequest`](crate::ConfirmRequest) across the
    /// suspension. Accepting hands it back to the wrapped effect; declining drops it
    /// with the rest of the offer. One [`Effect`] still declares at most **one** group,
    /// so the catalog validator rejects a `may` over two targeting effects
    /// ([`Violation::TwoTargetsInsideOptional`](crate::Violation)) rather than letting
    /// a card silently do half of what it says.
    ///
    /// A target that has become illegal by the time the object resolves takes the
    /// ordinary CR 608.2b path: an object whose every target is illegal never resolves
    /// and the question is never asked at all.
    May {
        /// **Who is asked** — the controller for every `you may`, and the player the
        /// sentence names for an `unless *that player* …`.
        ///
        /// Demanding Dragon is why this is a field: `deals 5 damage to target opponent
        /// unless that player sacrifices a creature` is a decision, and it is not the
        /// Dragon's controller's. Everything else about the offer is unchanged — the same
        /// question, the same payment, the same queue — because who answers a question was
        /// already a property of the queue rather than of the effect.
        ///
        /// A [`PlayerRef`] that names a target reads the offer's own chosen target, so
        /// "that player" and "target opponent" are the same seat here and the card may
        /// write whichever it prints.
        #[serde(default = "PlayerRef::controller")]
        chooser: PlayerRef,
        /// What accepting charges, or absent for a plain `you may …` that asks only for
        /// a yes.
        ///
        /// The [`OptionalCost`] vocabulary: mana, a permanent the chooser picks, or a
        /// discard. A mana cost is charged from the pool as the answer is given; the
        /// other two are *questions of their own*, posed on acceptance and answered
        /// before the wrapped effects run, so the payment always precedes what it bought.
        /// Whichever it is, a cost the controller could not pay is never posed at all
        /// (see below) and a player asked to pay may still activate mana abilities
        /// (CR 605.3a) — the one thing that stays legal while a choice is owed.
        #[serde(default)]
        cost: Option<OptionalCost>,
        /// What happens on acceptance, applied in order and in the surrounding
        /// object's frame — the same controller and the same source permanent the
        /// enclosing effects resolve in.
        effects: Vec<Effect>,
        /// What happens **instead**, when the offer is declined — the `unless` half of
        /// `sacrifice it unless you pay {1}`. Empty for a plain `you may`, where
        /// declining means nothing happens at all.
        ///
        /// It is the same effect list in the same frame, so the branch not taken costs
        /// nothing to express: a card with a consequence and a card without one are one
        /// shape with one field different.
        ///
        /// Its presence changes when the question is **asked**. A `you may pay` whose
        /// cost the controller cannot afford is normally not posed at all — there is no
        /// decision — but with a consequence attached there is still something to
        /// happen, so the branch is taken rather than the effect skipped. A player who
        /// cannot pay is not asked; they are told.
        ///
        /// It may target — but only when the accepted branch does not, and then the
        /// targets are its own. One announcement chooses one set of targets (CR 601.2c),
        /// and the branch that declared the slot is the branch they were chosen for: an
        /// accepted offer's targets go to what it wraps, a declined `unless` branch's go
        /// to the consequence, and neither branch ever sees the other's. A card that
        /// aimed on both sides would be two announcements nobody made, which is what the
        /// catalog validator still rejects.
        #[serde(default)]
        otherwise: Vec<Effect>,
    },
    /// **You get an emblem with** `abilities` (CR 114) — the planeswalker ultimate's
    /// verb, and the only way an [`Emblem`](crate::Emblem) is ever created.
    ///
    /// An emblem is a zoneless object with no characteristics but its abilities
    /// (CR 114.1–114.4). It is not a permanent, has no [`PermanentId`], is in no zone,
    /// and nothing in the game removes it — so unlike every other object the effect IR
    /// creates, this one is permanent in the strongest sense: the planeswalker that
    /// made it may die in the same turn and the emblem keeps going for the rest of the
    /// game.
    ///
    /// The abilities are authored inline, exactly as an [`Effect::CreateToken`]'s
    /// [`TokenData`] is, for the same reason: an emblem is not a card, so there is no
    /// catalog entry to point at. Only [`Ability::Static`] and [`Ability::Triggered`]
    /// reach it — an activated ability would need a way to be activated and a
    /// self-replacement would need an entry event to replace, neither of which an
    /// object outside every zone has. The catalog validator rejects the others.
    CreateEmblem {
        /// The abilities the emblem has, and its only characteristics (CR 114.1).
        abilities: Vec<Ability>,
        /// Who gets it, and therefore whose "you" its abilities are written from.
        /// Defaults to the effect's controller ("you get an emblem …").
        #[serde(default = "PlayerRef::controller")]
        player_ref: PlayerRef,
    },
    /// Apply `then` when `condition` holds as this effect is reached, `otherwise`
    /// when it does not — the *if* clause of `Draw a card. If you control three or
    /// more artifacts, draw two cards instead.`
    ///
    /// The condition is evaluated **at the moment this effect resolves**, not when the
    /// object was put on the stack, so it sees everything the effects before it did.
    /// That is what makes `Mill three cards. If at least one Zombie card was milled
    /// this way, …` expressible: the mill happens, then the condition reads what the
    /// mill recorded ([`Condition::MilledThisWay`]).
    ///
    /// The branch is **spliced into the remaining effect list** rather than applied
    /// here, so a branch may itself pose a player choice, suspend, and resume through
    /// the one path every other effect uses. Like [`Effect::May`], a branch may not
    /// choose a target — one effect declares at most one target slot, and a wrapper
    /// cannot honestly declare the slots of what it wraps ([`Effect::target_group`]);
    /// the catalog validator rejects one that tries.
    Conditional {
        /// What has to be true.
        condition: Condition,
        /// The effects applied when it is, in order.
        then: Vec<Effect>,
        /// The effects applied when it is not. Empty for a plain "if …, then …".
        #[serde(default)]
        otherwise: Vec<Effect>,
    },
    /// Return the single **card in a graveyard** this effect targets to the
    /// battlefield under the effect's controller (`Return target creature card with
    /// mana value 2 or less from your graveyard to the battlefield.`).
    ///
    /// The first effect whose target is a [`Target::Card`] rather than a permanent or
    /// a player: a card in a graveyard is a public object with an identity but no
    /// [`PermanentId`], so its spec ([`TargetSpec::CreatureCardInYourGraveyard`])
    /// selects over that zone. It enters through the one card→battlefield seam
    /// ([`GameState::put_card_onto_battlefield`](crate::GameState)), so it mints a
    /// fresh id and fires its enters-the-battlefield replacements and triggers exactly
    /// as a resolving permanent spell does.
    ReturnCardToBattlefield {
        /// What this effect is allowed to target (a card in a graveyard).
        target: TargetSpec,
        /// How many of them may be targeted — the `up to two creature cards from
        /// graveyards` of a planeswalker's ultimate. Defaults to exactly one.
        #[serde(default)]
        targets: TargetCount,
        /// Whether it arrives **tapped** — the creating effect's say, exactly as it is
        /// for a token (CR 111.1). Defaults to untapped.
        #[serde(default)]
        tapped: bool,
        /// Card types the permanent it makes gains (CR 613 layer 4), in addition to its
        /// own — the `That creature is a black **Zombie** in addition to its other colors
        /// and types` a reanimation spell prints as its second sentence.
        #[serde(default)]
        types: Vec<crate::card_type::CardType>,
        /// Subtypes it gains.
        #[serde(default)]
        subtypes: Vec<String>,
        /// Colours it gains (CR 613 layer 5).
        ///
        /// The three ride this effect rather than a separate `animate` beside it because
        /// they are about **the permanent this effect just made**, which nothing else can
        /// name: it did not exist when the spell was cast, so no target could have been
        /// chosen for it. The continuous effect lasts as long as that permanent does.
        #[serde(default)]
        colors: Vec<crate::mana::Color>,
        /// Counters the permanent enters with — the `with a corpse counter on it` of a
        /// reanimation that marks what it brought back.
        ///
        /// Placed as part of *entering* (CR 614.12), so they are already there before any
        /// state-based action or entry trigger looks — the same road
        /// [`Ability::EntersWithCounters`](crate::Ability) takes, for the same reason.
        #[serde(default)]
        counters: Vec<(CounterKind, u32)>,
        /// Whether the permanent is **exiled instead of going anywhere else** when it
        /// would leave the battlefield — `if that creature would leave the battlefield,
        /// exile it instead of putting it anywhere else`.
        ///
        /// A replacement effect (CR 614.1a) keyed to the permanent this effect just made,
        /// which nothing else could name: it did not exist when the ability was aimed. It
        /// lasts as long as the permanent does, and outlives the source that created it —
        /// the printed sentence sets no duration, so a creature reanimated this way is
        /// still exiled after the reanimator itself has died.
        #[serde(default)]
        exile_on_leaving: bool,
    },
    /// Return the **card in a graveyard** this effect targets to its owner's **hand**
    /// (`Return target creature card from your graveyard to your hand.`) — the
    /// graveyard→hand counterpart of [`Effect::ReturnCardToBattlefield`], and the
    /// card-in-a-zone counterpart of [`Effect::ReturnToHand`], which bounces a
    /// permanent.
    ///
    /// Its target is a [`Target::Card`] against a [`TargetSpec::CardInGraveyard`], so
    /// the class of card and whose graveyard it sits in are the spec's business rather
    /// than this effect's. The card goes to its **owner's** hand (CR 400.7), which for
    /// every card in a graveyard is the player whose graveyard that is.
    ///
    /// The one effect here that may name **more than one** target beside
    /// [`Effect::PutCounters`]: `return up to two target creature cards from your
    /// graveyard to your hand` is one effect with a two-slot group, applied once per
    /// target still legal on resolution.
    ReturnCardToHand {
        /// What each of this effect's slots is allowed to target (a card in a
        /// graveyard).
        target: TargetSpec,
        /// How many targets are chosen. Defaults to exactly one.
        #[serde(default)]
        targets: TargetCount,
    },
    /// Give the single creature this effect targets `power_per`/`toughness_per` **per
    /// permanent** matching `count_of`, until end of turn — the count-derived
    /// counterpart of [`Effect::Pump`] (`Target creature gets -X/-X until end of turn,
    /// where X is the number of Zombies you control.`).
    ///
    /// X is computed **on resolution** (CR 608.2), once, and the resulting fixed
    /// modifier is what the layer system folds in: a Zombie that dies later in the turn
    /// does not give the shrunk creature its toughness back, which is what the printed
    /// card means and what a re-evaluated selector would get wrong.
    PumpByCount {
        /// What this effect is allowed to target (a creature).
        target: TargetSpec,
        /// The signed power change contributed by each counted permanent.
        power_per: i32,
        /// The signed toughness change contributed by each counted permanent.
        toughness_per: i32,
        /// Which permanents are counted, relative to the effect's controller.
        count_of: PermanentCount,
    },
    /// Give the single creature this effect targets `power_per`/`toughness_per` **per
    /// unit of** `amount`, until end of turn — `Target creature gets -X/-X until end of
    /// turn, where X is the amount of life you gained this turn.`
    ///
    /// The sibling of [`Effect::PumpByCount`] for every X that is *not* a count of
    /// permanents ([`DerivedAmount`]), and identical to it in the one way that matters:
    /// X is computed **on resolution** (CR 608.2), once, and the fixed modifier that
    /// results is what the layer system folds in. Life gained after this resolves does
    /// not shrink the creature further, which is what the printed card means.
    ///
    /// A separate variant rather than a second field on [`Effect::PumpByCount`] because
    /// exactly one of the two sources is ever present: a card says "for each Zombie you
    /// control" *or* "where X is the amount of life you gained this turn", never both,
    /// and two optional fields would make "neither" and "both" authorable shapes that
    /// mean nothing.
    PumpByAmount {
        /// What this effect is allowed to target (a creature).
        target: TargetSpec,
        /// The signed power change contributed by each unit of the amount.
        power_per: i32,
        /// The signed toughness change contributed by each unit of the amount.
        toughness_per: i32,
        /// Where X comes from.
        amount: DerivedAmount,
    },
    /// The controller draws a number of cards the card does not print — `Draw cards equal
    /// to the greatest mana value among artifacts you control.`, and `you draw a card for
    /// each land card put into their graveyard this way`.
    ///
    /// The derived-amount counterpart of [`Effect::DrawCard`], whose `count` is a printed
    /// number. The subject is the same implicit one — the controller — so this effect
    /// chooses no target either, and each draw goes through the same
    /// [`Player::draw`](crate::Player::draw) seam, so emptying a library still flags the
    /// CR 704.5c decking loss.
    ///
    /// X is taken once, as the effect applies (CR 608.2), which for a "this way" source
    /// is what makes it read the mill the *same resolution* performed a moment earlier.
    DrawCardsByAmount {
        /// How many cards are drawn.
        amount: DerivedAmount,
    },
    /// The referenced player loses a number of life the card does not print — `Each
    /// player loses half their life, rounded up.`
    ///
    /// The derived-amount counterpart of [`Effect::LoseLife`], whose `amount` is a
    /// printed number, and the sibling of [`Effect::DrawCardsByAmount`] on the life
    /// total. It reaches the same [`Effect::LoseLife`] seam, so the loss still drives the
    /// CR 704.5a state-based action.
    ///
    /// The amount is read **once per named player, as the effect applies** (CR 608.2),
    /// and it is read *of that player*: `half their life` is each seat's own half, so a
    /// reference naming three seats reads three different numbers rather than the
    /// controller's one three times.
    LoseLifeByAmount {
        /// Which player loses the life.
        player_ref: PlayerRef,
        /// How much they lose.
        amount: DerivedAmount,
    },
    /// The referenced player **discards** a number of cards the card does not print —
    /// `Each player … discards half the cards in their hand, rounded up.`
    ///
    /// The derived-amount counterpart of [`Effect::Discard`], and deliberately the
    /// narrow one: the cards are picked by the player discarding them and any card in the
    /// hand may be picked. A coercive or filtered discard of a derived number is a card
    /// nobody has printed, and giving this variant the other two fields would make it
    /// authorable ahead of the card that means it.
    ///
    /// Like the fixed-count discard it poses one mid-resolution choice per named player
    /// ([`crate::PendingChoice`]) and suspends. The number is fixed when the choice is
    /// *posed*, from the hand as it stands then (CR 608.2), so a hand the same resolution
    /// already emptied asks for nothing.
    DiscardByAmount {
        /// Whose hands the cards leave.
        player_ref: PlayerRef,
        /// How many each of them discards.
        amount: DerivedAmount,
    },
    /// The referenced player **sacrifices** that many permanents they control (CR 701.17)
    /// — `Each player … sacrifices half the creatures they control, rounded up.`
    ///
    /// The first sacrifice in the IR that is an *effect* rather than a cost, and the two
    /// are not the same shape. A cost is paid as the spell is cast or the ability is
    /// activated, so the permanent rides on the action; this happens in the middle of a
    /// resolution, so it poses a mid-resolution choice ([`crate::PendingChoice`]) and
    /// suspends exactly as a discard does — and it is the only choice in the IR whose
    /// candidates are *permanents*, which is why a token can be picked.
    ///
    /// **Whose permanents is not a field.** CR 701.17b lets a player sacrifice only what
    /// they control, so the class is always the named player's own; `card_type` narrows
    /// it to one printed type and nothing more. The count is fixed when the choice is
    /// posed (CR 608.2) and clamped to what is actually there, so a player with one
    /// creature asked for two sacrifices the one.
    ///
    /// **An absent [`amount`](Self::Sacrifice::amount) is the open form** — `Sacrifice any
    /// number of lands` — and it is the same question with different bounds rather than a
    /// second verb: a floor of none and a ceiling of the whole class. That shape belongs
    /// here and not to a cost, because how many to sacrifice is a *decision*, and a
    /// decision needs a resolution to be asked during: Scapeshift's lands go when it
    /// resolves, so countering it takes none of them.
    Sacrifice {
        /// Which player sacrifices.
        player_ref: PlayerRef,
        /// How many permanents they sacrifice, or `None` for **any number they choose**,
        /// including none.
        #[serde(default)]
        amount: Option<DerivedAmount>,
        /// Restrict the choice to permanents with this printed card type. Absent lets
        /// any permanent they control be picked.
        #[serde(default)]
        card_type: Option<CardType>,
    },
    /// The referenced player gains `amount_per` life **per permanent** matching
    /// `count_of` (`You gain 1 life for each creature you control.`) — the
    /// count-derived counterpart of [`Effect::GainLife`], and the life-total sibling of
    /// [`Effect::PumpByCount`].
    ///
    /// X is computed **on resolution** (CR 608.2), once, from the board as it stands
    /// then: a creature that dies afterwards does not take the life back, which is what
    /// the printed card means. The count is relative to the effect's *controller* even
    /// when the life goes to someone else, because "each creature you control" says
    /// "you" and the subject clause does not change who that is.
    GainLifeByCount {
        /// Which player gains the life.
        player_ref: PlayerRef,
        /// How much life each counted permanent is worth.
        amount_per: u32,
        /// Which permanents are counted, relative to the effect's controller.
        count_of: PermanentCount,
    },
    /// Deal `amount_per` damage **per permanent** matching `count_of` to what
    /// [`DamageSubject`] names (`This creature deals damage to target creature an
    /// opponent controls equal to the number of Goblins you control.`) — the
    /// count-derived counterpart of [`Effect::DealDamage`], which it matches in every
    /// other respect including how the subject decides whether a target is chosen.
    ///
    /// Like [`Self::GainLifeByCount`] the count is taken once, on resolution, and is
    /// relative to the effect's controller.
    DealDamageByCount {
        /// Who or what takes the damage — one chosen target, or a class.
        #[serde(flatten)]
        subject: DamageSubject,
        /// How much damage each counted permanent is worth.
        amount_per: u32,
        /// Which permanents are counted, relative to the effect's controller.
        count_of: PermanentCount,
    },
    /// Deal damage equal to a [`DerivedAmount`] to what [`DamageSubject`] names —
    /// `deals X damage to any target`, where X is the value announced as the spell was
    /// cast (CR 601.2b), and `Thud deals damage equal to the sacrificed creature's power
    /// to any target.`
    ///
    /// The [`DerivedAmount`] counterpart of [`Effect::DealDamage`], and the sibling of
    /// [`Effect::DealDamageByCount`] for every X that is not a count of permanents,
    /// exactly as [`Effect::PumpByAmount`] is [`Effect::PumpByCount`]'s — a separate
    /// variant for the same reason: a card names one source or the other, never both, and
    /// two optional fields would make "neither" and "both" authorable shapes that mean
    /// nothing. The subject decides on its own whether a target is chosen, like every
    /// other damage verb.
    ///
    /// The amount is read **once**, where the effect applies (CR 608.2). For an announced
    /// X that reading is a lookup of a number already fixed — which is the whole point of
    /// announcing it: payment and resolution cannot disagree about a value neither of them
    /// computed. For an amount read off the object's own cost payment it is the moment the
    /// *stored* number is read, not the moment it was decided — the payment happened at
    /// announcement, and the creature it spent is already gone.
    DealDamageByAmount {
        /// Who or what takes the damage — one chosen target, or a class.
        #[serde(flatten)]
        subject: DamageSubject,
        /// Where the amount comes from.
        amount: DerivedAmount,
    },
    /// **Exile the referenced player's whole graveyard** (`Exile target player's
    /// graveyard.`) — the graveyard-hate verb.
    ///
    /// The subject is a [`PlayerRef`] exactly as [`Effect::Mill`]'s is, and decides on
    /// its own whether a target is chosen. Every card in that graveyard moves to its
    /// owner's exile zone at once; an already-empty graveyard is a legal target and a
    /// resolution that does nothing, which is what the printed card says.
    ExileGraveyard {
        /// Whose graveyard is exiled.
        player_ref: PlayerRef,
    },
    /// Put the single permanent this effect targets **on top of its owner's library**
    /// (`Put target nonland permanent on top of its owner's library.`) — the third
    /// destination beside [`Effect::ReturnToHand`]'s hand and [`Effect::Exile`]'s exile,
    /// and the harshest of the three, since the card is both gone and in the way.
    ///
    /// Like [`Effect::Tap`] the subject is an explicit target, chosen at cast
    /// (CR 601.2c) and re-checked on resolution (CR 608.2b). A **token** put anywhere
    /// but the battlefield ceases to exist (CR 111.7), so it never reaches the library.
    PutOnTopOfLibrary {
        /// What this effect is allowed to target.
        target: TargetSpec,
    },
    /// Add `amount` mana of `color` to the controller's pool that may be spent only as
    /// `restriction` allows (`Add {R}{R}. Spend this mana only to cast Dragon
    /// spells.`).
    ///
    /// The restricted counterpart of [`Effect::AddMana`], and a mana effect in exactly
    /// the same sense — an activated ability whose every effect is one of the three
    /// mana verbs is a mana ability ([`is_mana_ability`]) and never uses the stack
    /// (CR 605.1a). The restriction rides on the mana itself
    /// ([`RestrictedMana`](crate::RestrictedMana)) rather than on the pool, so it
    /// survives beside ordinary mana and empties with it at the end of the step
    /// (CR 500.4).
    AddRestrictedMana {
        /// The color of mana produced.
        color: Color,
        /// How much of it is produced.
        amount: u8,
        /// What that mana may be spent on.
        restriction: ManaRestriction,
    },
    /// **Attach this effect's own source** to the single permanent it targets — the whole
    /// of the equip action (CR 702.6b), and the only effect in the IR that moves one
    /// permanent onto another.
    ///
    /// Two subjects, and only one of them is chosen. The *host* is a target, picked on
    /// activation (CR 601.2c) and re-checked on resolution (CR 608.2b) — an equip whose
    /// creature died in response does nothing, and the Equipment stays exactly where it
    /// was. The *attachment* is the ability's own source, never named and never chosen,
    /// which is what makes "equip something else's Equipment" unsayable rather than
    /// merely unoffered.
    ///
    /// Attaching is a **move**, not an addition: an Equipment already attached to a
    /// creature becomes unattached from it and attached to the new one in the same step
    /// (CR 701.3c), so a second equip re-points the one field rather than accumulating.
    /// The grant that follows is read off the attachment by the layer system (CR 613
    /// layers 6 and 7c) and is therefore already correct on the new host and already gone
    /// from the old one, with nothing to move alongside it (ADR 0005).
    ///
    /// It is authored nowhere: the equip ability is derived from an Equipment's attachment
    /// block ([`equip_ability`](crate::card::equip_ability)), so this variant is reachable
    /// only through a card that actually is one.
    Attach {
        /// What this effect is allowed to attach its source to — an Equipment's
        /// [`attach_to`](crate::Attachment::attach_to), which for every printed Equipment
        /// is a creature its controller controls (CR 702.6b).
        target: TargetSpec,
    },
    /// Let the referenced player cast cards matching `filter` **from their graveyard**
    /// for the rest of the turn (`You may cast Zombie creature spells from your
    /// graveyard this turn.`).
    ///
    /// A permission, not a movement: the cards stay in the graveyard and are offered
    /// by [`valid_actions`](crate::valid_actions) alongside the hand, cast through the
    /// same [`Action::CastSpell`](crate::Action) and the same stack object. It lapses
    /// when the turn ends, so it is recorded with the turn it was granted on
    /// ([`GraveyardCasting`](crate::GraveyardCasting)) rather than with a duration to
    /// tick down.
    AllowCastingFromGraveyard {
        /// Whose graveyard becomes castable. Defaults to the effect's controller.
        #[serde(default = "PlayerRef::controller")]
        player_ref: PlayerRef,
        /// Which of that graveyard's cards may be cast. Defaults to any of them.
        #[serde(default)]
        filter: CardFilter,
    },
    /// **Exile the top `count` cards of the controller's library, and let them play those
    /// cards for the rest of the turn** — Dark-Dweller Oracle's `{1}, Sacrifice a
    /// creature: Exile the top card of your library. Until end of turn, you may play that
    /// card.`
    ///
    /// One effect rather than an exile and a permission written side by side, and the
    /// reason is what the permission has to name. *That card* is the card this resolution
    /// exiled: only the effect that moved them knows which those are, and by the time a
    /// second effect ran they would be indistinguishable from everything else in the zone.
    /// So the grant records the instances ([`ExilePlaying`](crate::ExilePlaying)) and a
    /// card that reached exile any other way is untouched.
    ///
    /// **Play, not cast** (CR 116.2a): a land among them is played under the ordinary
    /// one-per-turn allowance and sorcery-speed window, and a spell is cast through the
    /// same action, stack object, cost, and timing gates as a cast from hand. Only the
    /// zone it leaves differs — the same promise
    /// [`Self::AllowCastingFromGraveyard`] makes.
    ///
    /// A library with fewer than `count` cards exiles what it has: this is not a draw and
    /// running out is not a loss (CR 701.3d).
    ExileTopForPlay {
        /// How many cards are exiled from the top. Defaults to one.
        #[serde(default = "one")]
        count: u8,
        /// Whether only **spells** may be cast from among them, rather than any card
        /// being playable.
        ///
        /// The difference between two printed sentences: *you may play that card* lets a
        /// land among them be played (CR 116.2a), and *you may cast spells from among
        /// them* does not. Apex of Power says the second, Dark-Dweller Oracle the first,
        /// and a land exiled by Apex simply stays in exile.
        #[serde(default)]
        cast_only: bool,
    },
    /// **Reveal the top card of your library; you may play it without paying its mana
    /// cost; if you don't, exile it** (CR 608.2f) — Djinn of Wishes.
    ///
    /// The card is played *during this resolution*, not under a permission that outlives
    /// it: the resolution suspends, the player is offered that one card's real cast or
    /// land play, and the branch they did not take is what the card says happens instead.
    /// See [`PlayCardRequest`](crate::PlayCardRequest) for why that offer is an action
    /// rather than an answer.
    ///
    /// An empty library reveals nothing and asks nothing, which is not a loss (CR 704.5b
    /// is about drawing).
    RevealTopAndMayPlay {
        /// Whether it is played without paying its mana cost.
        #[serde(default)]
        free: bool,
    },
    /// **A player exiles cards from the top of their library until one of `class` is
    /// exiled** — the digging half of Chaos Wand.
    ///
    /// The first effect that reaches **another player's** library. The exile is face up
    /// (CR 701.16a) and recorded with its identities
    /// ([`GameEvent::CardsExiled`](crate::GameEvent)), which is what lets the sentence that
    /// follows it find the cards again.
    ///
    /// A library holding none of that class is dug through to the bottom: *until* stops at
    /// the end of the library as well as at a match, and running a library out this way is
    /// not a loss (CR 704.5b is about drawing).
    ExileFromLibraryUntil {
        /// Whose library is dug through. Defaults to a targeted opponent, which is the
        /// only thing a printed card of this shape says.
        #[serde(default = "PlayerRef::target_opponent")]
        player_ref: PlayerRef,
        /// The class the digging stops at.
        #[serde(default)]
        class: crate::ability::GraveyardCardClass,
    },
    /// **You may cast the card this resolution just exiled; then the rest go on the bottom
    /// of their owner's library in a random order** (CR 608.2f) — the second half of Chaos
    /// Wand, and its own effect because the first half has to *happen* before this one
    /// knows what to offer.
    ///
    /// It finds them the way every other *…this way* question does: by reading the log over
    /// this resolution's own window ([`GameEvent::CardsExiled`](crate::GameEvent)). The
    /// exile zone itself could not answer — it cannot tell a card this resolution put there
    /// from one that was already in it — and the resolution carries no list, which is what
    /// keeps [`Resolution`](crate::Resolution) a `Copy` of plain numbers.
    ///
    /// The card offered is the **last** one exiled, which is the one the digging stopped
    /// at; everything before it is what goes back. The caster is this effect's controller
    /// and the owner is whoever was dug through, which is ordinary Magic (CR 108.4).
    MayCastExiledThisWay {
        /// The class *that card* names — the same one the dig stopped at.
        ///
        /// Stated here rather than inferred from the dig, because it is what decides
        /// whether there is an offer at all: a library holding none of the class is dug to
        /// the bottom, and the last card turned over is then just the bottom card rather
        /// than the thing the sentence was about. Without this, running a library out would
        /// offer whatever happened to be underneath.
        #[serde(default)]
        class: crate::ability::GraveyardCardClass,
        /// Whether it is cast **without paying its mana cost**.
        #[serde(default)]
        free: bool,
    },
    /// Let the referenced player aim spells and abilities **as though hexproof were not
    /// there** for the rest of the turn (`Creatures your opponents control with hexproof
    /// can be the targets of spells and abilities you control as though they didn't have
    /// hexproof.`).
    ///
    /// A permission, exactly like [`Self::AllowCastingFromGraveyard`], and recorded the
    /// same way: per player, with the turn it was granted on
    /// ([`IgnoringHexproof`](crate::IgnoringHexproof)), dropped at the turn boundary.
    ///
    /// It names no permanent and no class of permanent because hexproof is already
    /// relative to who is aiming (CR 702.11b): the permission's holder is the only
    /// player it can change anything for, and their opponents' hexproof creatures are
    /// the only permanents it can change anything about. It is consulted in the single
    /// predicate that enforces hexproof, which both the announcement gate and the
    /// CR 608.2b resolution re-check run.
    IgnoreHexproof {
        /// Whose spells and abilities ignore hexproof. Defaults to the effect's
        /// controller.
        #[serde(default = "PlayerRef::controller")]
        player_ref: PlayerRef,
    },
    /// Create a **one-shot replacement effect** for the rest of the turn (CR 614.1b) —
    /// the `The next time a … would … this turn, … instead` of a printed card.
    ///
    /// The third per-turn thing an ability can put into the state, and recorded exactly
    /// like the two permissions above: on a list carrying the turn it was created on
    /// ([`PendingReplacement`](crate::PendingReplacement)), dropped at the turn boundary.
    /// It differs from them in one way, and it is the `next time`: applying it *removes*
    /// it, so a replacement that has done its job cannot do it twice.
    ///
    /// It names no target and no player. A replacement watches an **event**, and which
    /// events it watches is [`ReplacementEffect`](crate::ReplacementEffect)'s own filter
    /// — a class of thing that might happen, chosen when the card was written rather than
    /// aimed when the ability was activated.
    CreateReplacement {
        /// The replacement effect to create: what it watches, and what happens instead.
        replacement: crate::replacement::ReplacementEffect,
    },
    /// Raise a **damage-prevention shield** for the rest of the turn (CR 615.1) —
    /// `Prevent all combat damage that would be dealt this turn.`
    ///
    /// The prevention half of [`Self::CreateReplacement`], and separate from it for the
    /// one reason that matters: a shield is **not** one-shot. `The next time …` is spent
    /// by applying it; `this turn` is not, and covers every damage event until the turn
    /// ends. So it is recorded on [`GameState::prevention`](crate::GameState) with a
    /// duration rather than on the one-shot list, and it ends in the **cleanup step**,
    /// simultaneously with the marked damage and the pumps (CR 514.2), instead of at the
    /// turn boundary.
    ///
    /// Like a replacement it names no target and no player: a shield watches an
    /// **event**, and which events it covers is [`DamageFilter`](crate::DamageFilter) —
    /// chosen when the card was written, not aimed when the spell was cast. It applies to
    /// damage *anyone* would deal, which is what `all combat damage` says.
    PreventDamage {
        /// Which damage the shield prevents. An absent filter prevents all of it.
        #[serde(default)]
        damage: crate::replacement::DamageFilter,
    },
    /// Move **this ability's own card** out of the graveyard it is in
    /// (`Return this card from your graveyard to the battlefield tapped.`).
    ///
    /// The self-referential counterpart of [`Effect::ReturnCardToBattlefield`] and
    /// [`Effect::ReturnCardToHand`], which each target a card someone chose. Here the
    /// subject is the ability's own source (CR 115.1 — a source is not a target), so this
    /// fills no slot and can never fizzle, exactly as [`Effect::PumpSelf`] cannot.
    ///
    /// It is the effect that makes an ability **function from a graveyard** (CR 113.6):
    /// nothing else in the IR acts on a source that is a card in a zone rather than a
    /// permanent, so [`is_graveyard_ability`](crate::is_graveyard_ability) reads this
    /// variant and the offer, the activation, and the apply-time re-check all follow from
    /// it. Written on an ability of a permanent it does nothing — the source is on the
    /// battlefield, not in a graveyard — which is why the catalog validator rejects that
    /// authoring rather than letting the card quietly do nothing.
    ///
    /// The card leaves through the same graveyard→zone seams a *targeted* return uses, so
    /// a card that comes back to the battlefield mints a fresh
    /// [`PermanentId`](crate::PermanentId) and fires its enters-the-battlefield
    /// replacements and triggers like any other arrival, and one that comes back to a hand
    /// goes to its **owner's** (CR 400.7).
    ReturnSelfFromGraveyard {
        /// Where the card goes. The three destinations are the ones a printed card of
        /// this shape names: a hand, the battlefield, or the battlefield tapped.
        destination: FoundDestination,
    },
    /// **Shuffle this ability's own source into its owner's library** (CR 701.19) —
    /// `its owner shuffles it into their library`.
    ///
    /// The fourth destination beside [`Effect::ReturnToHand`]'s hand, [`Effect::Exile`]'s
    /// exile, and [`Effect::PutOnTopOfLibrary`]'s top of a library, and the only one that
    /// is not a place: a shuffled card is somewhere in the deck and nowhere in particular,
    /// which is what separates this from putting it on top. The shuffle draws from the
    /// seeded stream (ADR 0006), so a game replays identically through it.
    ///
    /// Self-referential, like [`Effect::PumpSelf`]: the subject is the ability's own
    /// source, which is not a target (CR 115.1), so this fills no slot and can never
    /// fizzle. A source that has already left the battlefield is not there to move, and
    /// the effect does nothing. A **token** ceases to exist on the way (CR 111.7) and
    /// never reaches the library; the library is still shuffled, because the instruction
    /// was to shuffle.
    ShuffleSelfIntoLibrary,
    /// **Transform** this ability's own source (CR 701.28a): the permanent turns over,
    /// and the face that was down is now up.
    ///
    /// The primitive CR 712.a is about, and the whole of it is that *nothing else
    /// happens*. The permanent is the same object: its counters, its marked damage, the
    /// Auras and Equipment attached to it, whether it is attacking or blocking, and the
    /// turn it came under its controller's control all survive, because turning it over
    /// changes one field ([`Printed::Card`](crate::Printed)'s face) and touches no other.
    /// It is not a zone change, so no [`PermanentId`](crate::PermanentId) is minted, no
    /// enters-the-battlefield trigger fires, and CR 400.7 never applies.
    ///
    /// A source that has no other face — a single-faced card, a token — is simply not
    /// turned over (CR 701.28d). Authored as `{"kind":"transform_self"}`.
    TransformSelf,
    /// Exile this ability's own source and return it to the battlefield **transformed**,
    /// under its owner's control — `Exile this, then return it transformed.`
    ///
    /// Deliberately **not** [`Self::TransformSelf`] with extra steps, and the difference
    /// is the one thing a player has to know: this is two zone changes, so what comes
    /// back is a *new object* (CR 400.7). It has a fresh
    /// [`PermanentId`](crate::PermanentId), no counters but the ones it enters with, no
    /// damage, nothing attached, no combat state, and summoning sickness. A planeswalker
    /// back face therefore arrives with its printed starting loyalty (CR 306.5b), which
    /// is exactly what makes the printed card work.
    ///
    /// The two halves go through the seams every other exile and every other arrival go
    /// through, so the entry runs the CR 614 replacement layer and is seen by the trigger
    /// diff like any other. Authored as `{"kind":"exile_self_and_return_transformed"}`.
    ExileSelfAndReturnTransformed,
    /// Exile every card in the targeted player's library **except the bottom one** —
    /// `Exile all but the bottom card of target player's library.`
    ///
    /// A library is hidden, so this is the one effect that moves a large number of unseen
    /// cards to a public zone at once. The bottom card is the library's first element,
    /// matching every other read of a library in the engine (the top is the last), and it
    /// stays where it is: a player left with one card draws once more before CR 704.5c
    /// takes the game.
    ExileLibraryExceptBottom {
        /// Whose library. Always a player reference that targets, since the printed card
        /// says `target player`.
        target: PlayerRef,
    },
    /// Create a **delayed triggered ability** for the rest of the turn (CR 603.7) — the
    /// `When you next … this turn, …` of a printed card.
    ///
    /// The fourth per-turn thing an ability can leave on the state, and recorded exactly
    /// like the three before it: on a list carrying the turn it was created on
    /// ([`PendingDelayedTrigger`](crate::PendingDelayedTrigger)), dropped at the turn
    /// boundary, and — like a created replacement — **spent by firing**, because `the
    /// next time` happens once (CR 603.7b).
    ///
    /// It names no target: a delayed ability watches an *event*, and the object it then
    /// acts on is whatever that event produced (CR 603.7c) rather than anything aimed
    /// when this resolved.
    CreateDelayedTrigger {
        /// What it waits for, and what it does when it fires.
        trigger: crate::delayed::DelayedTrigger,
    },
    /// **Copy the spell** this effect names (CR 707.10): put a copy of it onto the stack,
    /// above the original.
    ///
    /// A different operation from copying a permanent, and kept a different one. The copy
    /// is a new object on the stack that **was not cast** — no cast trigger fires for it,
    /// nothing that asks "was this cast" says yes, and it has no card, so when it finishes
    /// resolving it simply ceases to exist rather than reaching a graveyard (CR 707.10a).
    /// It acquires the original's characteristics *and* the decisions made for it
    /// (CR 707.10), which here means its chosen targets.
    ///
    /// The spell rides a target slot ([`TargetSpec::SpellOnStack`]), which for the delayed
    /// ability that creates today's only use is filled by the **trigger event** rather
    /// than by a player (CR 603.7c). The slot buys the CR 608.2b re-check: a spell that has
    /// been countered or has already resolved is no longer there to copy, and nothing
    /// happens — which is what CR 603.7c says in its own words.
    ///
    /// Only an instant or sorcery is copied. A copy of a **permanent** spell becomes a
    /// token as it resolves (CR 707.10f), and no token is created as a copy of anything
    /// here — so such a copy is not made at all rather than made wrongly.
    CopySpell {
        /// What may be copied. Always a spell on the stack.
        target: TargetSpec,
        /// Whether the copy's controller may choose new targets for it (CR 707.10c).
        #[serde(default)]
        new_targets: bool,
    },
    /// **You may pay `cost`. When you do,** the effects happen — as a *reflexive
    /// triggered ability* (CR 603.11), not as part of this resolution.
    ///
    /// The difference from [`Self::May`] is the whole reason this is its own effect, and
    /// it is visible on every card that prints it: the effects go on the **stack** after
    /// the payment, so their targets are chosen then (CR 603.11b) and both players get
    /// priority before they happen. Sparktongue Dragon's `you may pay {2}{R}. When you
    /// do, it deals 3 damage to any target` cannot choose that target when the Dragon is
    /// cast — nobody knows yet whether it will be paid for, and a target chosen up front
    /// would be a target chosen before the decision.
    ///
    /// The ability it creates is the source's own: `it deals 3 damage` is the Dragon
    /// dealing it, so the damage has a permanent as its source (CR 609.7).
    ///
    /// A cost the controller could not pay is never posed, exactly as [`Self::May`]'s is
    /// not — there is nothing to decide, and unlike an `unless` there is no consequence
    /// waiting on the other side.
    MayPayForTrigger {
        /// What paying costs. Always present: `when you do` refers to a payment, and an
        /// offer with nothing to pay would be a trigger with no condition.
        cost: OptionalCost,
        /// What the reflexive ability does when it resolves.
        effects: Vec<Effect>,
    },
    /// Create a **reflexive triggered ability** (CR 603.11) — the `When a creature is put
    /// onto the battlefield this way, …` a resolution says about something it has just
    /// done.
    ///
    /// The sibling of [`Self::CreateDelayedTrigger`], and the differences are the whole
    /// reason it is not that: a reflexive ability watches an event **inside the
    /// resolution that created it**, fires (or does not) before that transition ends, and
    /// then no longer exists — there is nothing to spend and nothing to clear at a turn
    /// boundary. See [`crate::reflexive`].
    ///
    /// Written **after** the effect it watches, in the same effect list, because it asks
    /// what that effect did: an authoring order that put it first would ask about a
    /// resolution that had not done anything yet, and get the honest answer.
    ///
    /// It names no target of its own. The ability it creates may, and that target is
    /// chosen when the ability goes on the stack (CR 603.3d) like any other trigger's —
    /// which is exactly the thing a `when you do` needs and an announcement could not
    /// have provided.
    CreateReflexiveTrigger {
        /// What it fires on, and what it does.
        trigger: crate::reflexive::ReflexiveTrigger,
    },
    /// The ability's **own source deals damage equal to its power** to the one permanent
    /// this effect targets — the `it deals damage equal to its power to target creature an
    /// opponent controls` of a reflexive trigger about a creature that just arrived.
    ///
    /// A self-referential effect, like [`Self::PumpSelf`]: the dealer is not chosen and is
    /// never named, because the sentence's subject is the object the ability is on. That
    /// is also what makes the damage's *source* a permanent (CR 609.7) rather than a
    /// spell.
    ///
    /// The power is read on **resolution** (CR 608.2), so a creature pumped in response
    /// deals the larger number. A source that is no longer on the battlefield deals its
    /// **last known** power instead (CR 608.2h) — killing the creature in response is the
    /// obvious answer to this trigger, and it does not stop the damage.
    SelfDealsDamage {
        /// What may be dealt to. One slot, chosen when the ability goes on the stack.
        target: TargetSpec,
    },
    /// The targeted permanent **becomes** a creature of the named types with a base power
    /// and toughness — Skilled Animator's `target artifact you control becomes an artifact
    /// creature with base power and toughness 5/5 for as long as this creature remains on
    /// the battlefield`.
    ///
    /// Two layers in one effect because the card is one sentence: the types are added at
    /// CR 613 layer 4 and the base P/T is set at 7b. Neither replaces anything — an
    /// artifact that becomes an artifact creature is still an artifact, which is exactly
    /// what "in addition" means, and the base P/T is a *base*, so counters and anthems
    /// fold onto it afterwards.
    ///
    /// The duration is what the printed card says. `for as long as this creature remains
    /// on the battlefield` is [`Duration::WhileOnBattlefield`](crate::Duration) keyed to
    /// the source, and `until end of turn` is the other one — the same two durations every
    /// continuous effect in the engine has, rather than a vocabulary of its own.
    Animate {
        /// What may be animated.
        target: TargetSpec,
        /// Card types it gains (CR 613 layer 4).
        #[serde(default)]
        types: Vec<crate::card_type::CardType>,
        /// Subtypes it gains.
        #[serde(default)]
        subtypes: Vec<String>,
        /// Colours it gains (CR 613 layer 5) — `is a **black** Zombie in addition to its
        /// other colors and types`. Added, never replaced.
        #[serde(default)]
        colors: Vec<crate::mana::Color>,
        /// The base power it becomes, when the card names one (CR 613 layer 7b).
        #[serde(default)]
        power: Option<i32>,
        /// The base toughness it becomes.
        #[serde(default)]
        toughness: Option<i32>,
        /// Whether the effect lasts only until end of turn. `false` is `for as long as`
        /// the source remains on the battlefield.
        #[serde(default)]
        until_end_of_turn: bool,
        /// Whether it lasts **until its controller's next turn** instead — the `until your
        /// next turn` of a planeswalker's zero ability.
        ///
        /// A third answer to one question, and a flag beside the other rather than a
        /// duration authored outright, because that is how the card reads: the effect is
        /// the same animation, and this says only how long it stands. Takes precedence
        /// over [`until_end_of_turn`](Self::Animate::until_end_of_turn), which no card
        /// authors alongside it.
        #[serde(default)]
        until_your_next_turn: bool,
    },
    /// Put **any number of cards from the controller's hand** onto the battlefield face
    /// down (CR 708.2), as `values` — Tezzeret, Cruel Machinist's ultimate.
    ///
    /// The open form of a hand selection, exactly as an absent amount is the open form of
    /// a sacrifice: a floor of none and a ceiling of the whole hand, so a player with an
    /// empty hand answers it with nothing rather than being stalled by it.
    ///
    /// What arrives is a card with the characteristics `values` names and none of its own
    /// — no name, no abilities, no colour it printed — which is what being face down is
    /// (CR 708.2). The card is still underneath, so one that dies reaches its owner's
    /// graveyard as itself rather than ceasing to exist (CR 708.4).
    PutHandOntoBattlefieldFaceDown {
        /// What each card becomes while it is face down.
        values: TokenData,
    },
    /// **Remove all counters from the targeted permanent, and forbid it any more** for as
    /// long as this effect's source remains on the battlefield — Suncleanser's first mode.
    ///
    /// Two halves of one sentence, and one effect rather than two because the second half
    /// is not something that happens: it is a continuous effect the first half's
    /// resolution creates (CR 611.2b), keyed to the same permanent and lasting as long as
    /// the source does. Splitting them would mean a second target slot naming the same
    /// object, which the card does not ask for.
    ///
    /// The prohibition is a
    /// [`RuleModification::CannotHaveCountersPut`](crate::RuleModification), applied at
    /// the one counter seam every road to a counter runs through — so it forbids a
    /// `+1/+1` counter from a spell, from an activated ability, and from a state-based
    /// action equally, without any of them knowing it exists.
    RemoveAllCounters {
        /// What may be aimed at.
        target: TargetSpec,
        /// Whether the permanent is also forbidden further counters for as long as the
        /// source remains on the battlefield.
        ///
        /// A field rather than an assumption because the two sentences are separable on a
        /// printed card: removing counters is a common effect and forbidding them is a
        /// rare one, and an effect that only clears counters authors `false`.
        #[serde(default)]
        then_forbid: bool,
    },
    /// **The player this effect names loses all counters, and is forbidden any more** for
    /// as long as the source remains on the battlefield — Suncleanser's second mode.
    ///
    /// The player-side twin of [`Self::RemoveAllCounters`], and identical in shape because
    /// a player's counters and a permanent's are one mechanism: the same removal, the same
    /// [`RuleModification::CannotHaveCountersPut`](crate::RuleModification), and the same
    /// single seam enforcing it.
    ///
    /// Nothing in the bundled catalog gives a player a counter, so today both halves are
    /// correct and inert: there is nothing to remove and nothing to forbid. That is a fact
    /// about the catalog rather than about this effect — the day a card hands out a poison
    /// counter, this stops it with no change here.
    PlayerLosesAllCounters {
        /// Whose counters — `target_opponent` on the one card that prints it.
        player_ref: PlayerRef,
        /// Whether that player is also forbidden further counters for as long as the
        /// source remains on the battlefield.
        #[serde(default)]
        then_forbid: bool,
    },
    /// **For each player, one chosen permanent that player controls is sacrificed** —
    /// and each player who lost one reveals the top card of their library and puts it
    /// onto the battlefield if it is a permanent card. Vaevictis Asmadi, the Dire's
    /// attack trigger.
    ///
    /// The only effect whose **number of target slots comes from the table** rather than
    /// from the card: it declares one required slot per seat, each naming that seat's
    /// permanents ([`TargetSpec::PermanentThatPlayerControls`]), which is why
    /// [`Self::target_groups`] is given a seat count at all. Nothing about it is written
    /// for two players — a three-seat game declares three slots and sacrifices three
    /// permanents, and the code that does it is the same loop.
    ///
    /// Everything the printed card conditions on the *sacrifice actually happening* is
    /// conditioned on it here too. A slot whose target became illegal between
    /// announcement and resolution (CR 608.2b) sacrifices nothing, and that player does
    /// not reveal — "each player who sacrificed a permanent **this way**". A player whose
    /// revealed card is not a permanent card has simply revealed it; it stays on top,
    /// because the card says *puts it onto the battlefield if it's a permanent card* and
    /// says nothing about anywhere else for it to go.
    SacrificeChosenPerPlayer {
        /// Whether each player who sacrificed then reveals the top card of their library
        /// and puts it onto the battlefield if it is a permanent card.
        ///
        /// A field rather than an assumption because the sacrifice and the replacement
        /// are two sentences on the printed card, and only one card prints both. An
        /// effect that wants the symmetrical sacrifice alone authors `false` and inherits
        /// the whole per-seat targeting pipeline.
        #[serde(default)]
        reveal_top: bool,
    },
    /// **Exchange control** of the two permanents this effect names (CR 701.10) —
    /// Switcheroo's `Exchange control of two target creatures.`
    ///
    /// The one effect beside [`Self::Fight`] whose two slots are not interchangeable in
    /// the sense that matters: each creature goes to the *other's* controller, so the
    /// pair is the effect and half of it is nothing. CR 701.10c says so outright — if
    /// either object is an illegal target, or the two are controlled by the same player,
    /// no control changes at all.
    ///
    /// It is written as two CR 613 layer-2 effects because that is what a control change
    /// is here, and an exchange is not a duration: the two swap **indefinitely**, so both
    /// last as long as their permanents do rather than until end of turn.
    ExchangeControl {
        /// What the first slot may name.
        first: TargetSpec,
        /// What the second may name. The same class on every printed card, and a field of
        /// its own for [`Self::Fight`]'s reason: two slots, stated separately.
        second: TargetSpec,
    },
    /// **Exile the permanent this effect targets until the source leaves the
    /// battlefield** (CR 610.3) — Hieromancer's Cage's `exile target nonland permanent an
    /// opponent controls until this enchantment leaves the battlefield`.
    ///
    /// The first **linked** effect in the engine: what was exiled and what exiled it are
    /// recorded together ([`ExiledUntil`](crate::ExiledUntil)), because the return is a
    /// sentence about *this* card and no snapshot of exile could say which exile it meant.
    ///
    /// The card comes back **when the source leaves**, whatever takes it — destroyed,
    /// bounced, exiled in turn — and comes back to the battlefield under its **owner's**
    /// control (CR 610.3b), not the Cage controller's. A source that leaves in the same
    /// action that exiled the card returns it immediately, which is the honest reading of
    /// a linked ability whose partner is already gone.
    ExileUntilSourceLeaves {
        /// What may be exiled.
        target: TargetSpec,
    },
    /// The referenced player **takes an extra turn** after this one (CR 720.1) —
    /// Magistrate's Scepter's payoff for three charge counters.
    ///
    /// The turn is queued rather than taken: the game finishes the turn it is in, and the
    /// rotation hands the next one to whoever this named
    /// ([`GameState::with_extra_turn`](crate::GameState)). Extra turns are taken **last in
    /// first out** (CR 720.1), which is already what that queue does — two of them granted
    /// in one turn are taken in the order the second, then the first.
    ///
    /// The subject is a non-targeted [`PlayerRef`], and every printed card that says this
    /// says "you". This effect chooses no target.
    TakeExtraTurn {
        /// Which player takes it.
        player_ref: PlayerRef,
    },
    /// **This ability's own source** becomes what the clause names — Ursine Champion's
    /// `becomes a Bear Berserker until end of turn` and Chromium's `becomes a Human with
    /// base power and toughness 1/1`.
    ///
    /// The self-referential [`Self::Animate`], and self-referential for the same reason
    /// [`Self::PumpSelf`] is: the source is not a target (CR 115.1), so this chooses
    /// nothing, fills no slot, and can never fizzle.
    ///
    /// `replace_subtypes` is the one thing it has that [`Self::Animate`] does not, and it
    /// is CR 205.1b: `becomes a Human` **sets** the creature types where `is a Knight in
    /// addition to its other types` adds one. A card that means the first and is authored
    /// as the second stays an Elder Dragon, which a Dragon lord would still pump.
    AnimateSelf {
        /// Card types it gains (CR 613 layer 4).
        #[serde(default)]
        types: Vec<crate::card_type::CardType>,
        /// Subtypes it takes on.
        #[serde(default)]
        subtypes: Vec<String>,
        /// Whether those subtypes **replace** the printed ones (CR 205.1b) rather than
        /// joining them.
        #[serde(default)]
        replace_subtypes: bool,
        /// Colours it gains (CR 613 layer 5).
        #[serde(default)]
        colors: Vec<crate::mana::Color>,
        /// The base power it becomes (CR 613 layer 7b).
        #[serde(default)]
        power: Option<i32>,
        /// The base toughness it becomes.
        #[serde(default)]
        toughness: Option<i32>,
    },
    /// **Sacrifice this ability's own source** (CR 701.17) — the `sacrifice it` of a land
    /// that asks for a toll on the way in.
    ///
    /// Self-referential, like [`Self::TapAttached`] beside it, and the counterpart of the
    /// `sacrifice_this` *cost*: a cost is paid as an ability is activated, and this
    /// happens in the middle of a resolution. It chooses nothing and asks nothing — the
    /// permanent is named by the sentence, so there is no class and no decision — which
    /// is exactly what separates it from [`Self::Sacrifice`].
    ///
    /// A source that has already left the battlefield sacrifices nothing.
    SacrificeSelf,
    /// **Tap the permanent this ability's source is attached to** (CR 303.4) — the `tap
    /// enchanted creature` of an Aura that arrives holding its host down.
    ///
    /// A self-referential effect, like [`Self::PumpSelf`], and it targets nothing: the
    /// card does not print the word *target*, because the Aura already chose what it
    /// enchants when it was cast (CR 601.2c). Aiming again would be a second choice the
    /// card never asks for, and one the player could point somewhere else.
    ///
    /// Does nothing for a source that is attached to nothing, or that is no longer on the
    /// battlefield at all — an Aura whose host left has no enchanted creature, and the
    /// sentence has nothing to be about.
    TapAttached,
    /// **Destroy the permanent this ability's source is attached to** (CR 701.7) —
    /// Dwindle's `when enchanted creature blocks, destroy it`.
    ///
    /// [`Self::TapAttached`]'s neighbour in every respect, including the one that matters:
    /// it targets nothing. The Aura chose what it enchants when it was cast (CR 601.2c),
    /// and the word "it" points back at that choice rather than at a new one.
    ///
    /// Destruction, not sacrifice — so an indestructible host survives it, and a
    /// regeneration shield would too if one existed. A source attached to nothing, or one
    /// that has left the battlefield, destroys nothing.
    DestroyAttached,
    /// The referenced player **wins the game** (CR 104.2b) — the payoff of a card that
    /// ends the game on its own terms rather than by reducing anyone to zero.
    ///
    /// The engine has no "has won" flag and deliberately gains none: a game ends when at
    /// most one player is still in it, and the survivor *is* the winner (CR 104.2a), so
    /// [`GameResult`](crate::GameResult) is derived from who has lost and nothing else.
    /// Winning is therefore expressed as what it does to everyone else — every other
    /// player in the game loses it, recorded with
    /// [`LossReason::OpponentWon`](crate::player::LossReason::OpponentWon) — which is
    /// also what the rules say happens at a table of any size. One derivation of the
    /// result, one place a game can end, and no second way to be finished that every
    /// reader of the state would have to learn.
    ///
    /// A player who has already lost cannot win (CR 104.3a — they are no longer in the
    /// game), so the effect does nothing for such a subject rather than resurrecting
    /// them.
    ///
    /// The subject is a non-targeted [`PlayerRef`], and every printed card that says this
    /// says "you". This effect chooses no target.
    WinTheGame {
        /// Which player wins.
        player_ref: PlayerRef,
    },
}
