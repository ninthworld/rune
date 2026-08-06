//! The [`Effect`] vocabulary: every single thing an ability or a spell can do, and the
//! questions asked about one.

use super::*;

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
    /// Add `amount` mana **in any combination of colors** — the player chooses each
    /// point's color as the effect resolves (`Add two mana in any combination of
    /// colors.`).
    ///
    /// The choice is real and it is per point: the player is asked once for each mana,
    /// so two mana may be two of one color or one each of two. The questions ride the
    /// ordinary mid-resolution choice queue ([`crate::ChoiceQuestion::Color`]), which
    /// is why an effect that looks like a variant of [`Effect::AddMana`] is a separate
    /// verb — the amount is fixed, but the *colors* are not authored at all.
    ///
    /// The optional `restriction` rides on every point produced, exactly as
    /// [`Effect::AddRestrictedMana`]'s does (CR 106.6), so `Add two mana in any
    /// combination of colors. Spend this mana only to cast Dragon spells.` is one
    /// effect rather than a colored one repeated five ways.
    AddManaAnyColor {
        /// How many mana are produced, and therefore how many colors are chosen.
        amount: u8,
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
    /// them matching `filter` into `destination`, and put the rest on the bottom in a
    /// random order — the "look at the top N and take one" verb.
    ///
    /// Taking is optional (every card printed this way says "you may"), so a look that
    /// turns up nothing legal still bottoms what it looked at rather than stalling. The
    /// unchosen cards are bottomed in a random order drawn from the seeded RNG
    /// ([`crate::GameState::rng_seed`]), which is what the cards printed with this
    /// wording say and what keeps the looked-at cards from becoming known future draws.
    LookAtTop {
        /// How many cards from the top are looked at.
        count: u8,
        /// How many of them may be taken.
        take: u8,
        /// Which of the looked-at cards may be taken. Defaults to any of them.
        #[serde(default)]
        filter: CardFilter,
        /// Where a taken card goes. Defaults to its owner's hand.
        #[serde(default)]
        destination: FoundDestination,
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
        take: u8,
        /// Which cards of the library may be found. Defaults to any of them.
        #[serde(default)]
        filter: CardFilter,
        /// Where a found card goes. Defaults to its owner's hand.
        #[serde(default)]
        destination: FoundDestination,
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
        #[serde(default = "one")]
        count: u8,
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
    /// `you may draw a card`, and `you may pay {1}. If you do, draw a card`.
    ///
    /// The first effect in the IR whose mid-resolution question is a *decision* rather
    /// than a selection (CR 608.2). It suspends resolution and asks the ability's
    /// **controller** — never the priority holder, and never a player the effect merely
    /// names — through the same [`PendingChoice`](crate::PendingChoice) queue a discard
    /// or a search goes through.
    ///
    /// Declining is not a fizzle: it skips these effects and nothing else, so an
    /// optional effect sitting between two mandatory ones leaves both of them intact.
    /// A cost the controller could not pay under any tapping is never posed at all —
    /// it is declined outright — and either way the log records that the question was
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
        /// The mana cost the controller pays to apply the effects, in the same
        /// `{...}` notation an activation cost is written in. Absent for a plain
        /// `you may …`, which asks only for a yes.
        ///
        /// Paid from the controller's mana pool through the one
        /// [`ManaPool::pay`](crate::ManaPool::pay) seam a cast and an activation use,
        /// so the three can never disagree about what a cost string means. A player
        /// asked to pay here may still activate mana abilities to do it (CR 605.3a),
        /// which is the one thing that stays legal while a choice is owed.
        #[serde(default)]
        cost: Option<String>,
        /// What happens on acceptance, applied in order and in the surrounding
        /// object's frame — the same controller and the same source permanent the
        /// enclosing effects resolve in.
        effects: Vec<Effect>,
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
        /// Whether it arrives **tapped** — the creating effect's say, exactly as it is
        /// for a token (CR 111.1). Defaults to untapped.
        #[serde(default)]
        tapped: bool,
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
}

impl Effect {
    /// The [`TargetSpec`] this effect's slot group names, or `None` for an effect with
    /// an implicit subject ([`Effect::AddMana`], [`Effect::DrawCard`]).
    ///
    /// The convenience over [`Self::target_group`] for the great majority of callers,
    /// which care what may be targeted and not how many.
    #[must_use]
    pub fn target_spec(&self) -> Option<TargetSpec> {
        self.target_group().map(|group| group.spec)
    }

    /// The [`TargetGroup`] this effect must be given chosen targets for, or
    /// `None` for an effect with an implicit subject ([`Effect::AddMana`],
    /// [`Effect::DrawCard`]).
    ///
    /// The resolution path uses this to pair each of an object's stored
    /// [`Target`]s with the effect that consumes it and to re-check that
    /// target's legality (CR 608.2b). Kept exhaustive so a new targeting
    /// [`Effect`] variant must declare its spec here.
    #[must_use]
    pub fn target_group(&self) -> Option<TargetGroup> {
        match self {
            // The one variable-arity effect: `put a +1/+1 counter on each of up to two
            // target creatures` chooses between zero and two, and every other authoring
            // of the same effect leaves the count at its default of one.
            Effect::PutCounters {
                target, targets, ..
            }
            | Effect::ReturnCardToHand { target, targets } => {
                Some(TargetGroup::counted(*target, *targets))
            }
            // An optional effect declares the group of the **one** effect it wraps, so
            // "you may destroy target artifact" names its target once, at announcement
            // (CR 601.2c), and the yes-or-no comes later. The wrapper is a forwarder
            // rather than a second slot: the same group is answered for here and by the
            // wrapped effect once it is spliced in, which is what keeps the flat stored
            // target list pairing back exactly. The catalog validator holds a `may` to
            // one such effect, so this is a lookup and never a choice between two.
            Effect::May { effects, .. } => effects.iter().find_map(Effect::target_group),
            Effect::Tap { target }
            | Effect::CounterSpell { target }
            | Effect::Destroy { target }
            | Effect::Exile { target }
            | Effect::Pump { target, .. }
            | Effect::PumpByCount { target, .. }
            | Effect::GrantKeyword { target, .. }
            | Effect::Restrict { target, .. }
            | Effect::GainControl { target, .. }
            | Effect::ReturnCardToBattlefield { target, .. }
            | Effect::PutOnTopOfLibrary { target }
            // An equip names its *host* as a target and its own source as everything
            // else, so it declares exactly one slot (CR 702.6b).
            | Effect::Attach { target }
            | Effect::ReturnToHand { target } => Some(TargetGroup::single(*target)),
            // A player-subject effect targets exactly when its reference does
            // (CR 115.1) — "target opponent loses 2 life" fills a slot, "each
            // opponent loses 2 life" fills none. One answer, from the reference.
            Effect::GainLife { player_ref, .. }
            | Effect::LoseLife { player_ref, .. }
            | Effect::Mill { player_ref, .. }
            // A discard names its hand the same way, and for the same reason: "target
            // player discards two cards" fills a slot and can fizzle, "each opponent
            // discards a card" fills none and cannot.
            | Effect::Discard { player_ref, .. }
            | Effect::GainLifeByCount { player_ref, .. }
            | Effect::ExileGraveyard { player_ref }
            // And a mass tap names whose creatures the same way: "tap all creatures
            // target player controls" fills a slot, and a class relative to the
            // controller would not.
            | Effect::TapAll { player_ref, .. }
            // And a token creation names its creator the same way: "create a 2/4 white
            // Ox token" is made by you, "target player creates …" fills a slot.
            | Effect::CreateToken { player_ref, .. } => {
                player_ref.target_spec().map(TargetGroup::single)
            }
            // Damage asks its subject the same question: "any target" fills a slot,
            // "each opponent" and "each creature" fill none (CR 115.1).
            Effect::DealDamage { subject, .. } | Effect::DealDamageByCount { subject, .. } => {
                subject.target_spec().map(TargetGroup::single)
            }
            Effect::AddMana { .. }
            | Effect::AddColorlessMana { .. }
            | Effect::AddRestrictedMana { .. }
            // A color choice names no target: the question is about mana, and the
            // player answering it is the effect's controller by definition.
            | Effect::AddManaAnyColor { .. }
            | Effect::DrawCard { .. }
            // An emblem is given to a named player, never a targeted one (CR 114.3 —
            // "you get an emblem"), and a graveyard-casting permission likewise names
            // its player without targeting.
            | Effect::CreateEmblem { .. }
            | Effect::AllowCastingFromGraveyard { .. }
            // A choice over the controller's own library names no target: the library
            // is theirs by definition (CR 115.1).
            | Effect::Scry { .. }
            | Effect::LookAtTop { .. }
            | Effect::SearchLibrary { .. }
            // A conditional declares no slot: it has two branches and one flat target
            // list, so a group named in either one could not be paired back onto the
            // branch that was actually taken. The catalog validator rejects a branch
            // that tries.
            | Effect::Conditional { .. }
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
