//! What an activated ability charges to activate (CR 118): tapping, mana, loyalty, and
//! the three costs whose payment the activating player **chooses** — plus the subset of
//! that vocabulary an optional effect may charge mid-resolution ([`OptionalCost`]).

use super::*;

/// How many permanents a sacrifice cost takes (CR 601.2b).
///
/// One vocabulary for both places a cost sacrifices something the player picks — an
/// activation's [`Cost::Sacrifice`] and a cast's
/// [`AdditionalCost::Sacrifice`](crate::AdditionalCost) — because "how many" is the same
/// question of both and a second spelling would be a second thing to get wrong.
///
/// Authored the way [`ObservedSpell`] is, externally tagged: `{"exactly": 2}`. Absent
/// defaults to exactly one, which is what every card written before this existed says.
///
/// **A cost is never a number the payer picks.** An open count — `sacrifice any number of
/// lands` — is a *decision*, and a decision belongs to a resolution the player can be asked
/// during ([`Effect::Sacrifice`](crate::Effect)), not to a payment made as the object goes
/// on the stack. Scapeshift was the only card that ever asked for one here, and it does not:
/// its sacrifice is part of its resolution, so countering it costs no lands at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SacrificeCount {
    /// Exactly this many, never fewer and never more — the `two artifacts` of an
    /// activation that eats a pair. Over-paying a cost is not something a player may
    /// choose to do, so this is exact in both directions.
    Exactly(u8),
}

impl Default for SacrificeCount {
    fn default() -> Self {
        Self::Exactly(1)
    }
}

impl SacrificeCount {
    /// The fewest permanents this count accepts — which is also the most, since every
    /// count a cost may name is exact.
    #[must_use]
    pub fn min(self) -> u8 {
        match self {
            Self::Exactly(count) => count,
        }
    }

    /// Whether `paid` permanents is exactly what this count asks for.
    ///
    /// A named predicate rather than a comparison at each caller, so the offer gate, the
    /// payment check, and the server's slot all read the count one way: met by that many,
    /// refused by fewer, and refused by more — over-paying a cost is not something a
    /// player may choose to do.
    #[must_use]
    pub fn is_paid_by(self, paid: usize) -> bool {
        match self {
            Self::Exactly(count) => paid == usize::from(count),
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
    /// Change the source's **loyalty** by `amount` — the cost of a planeswalker's
    /// loyalty ability (CR 606.1), written on the card as `+1`, `0`, or `−2` and
    /// authored as `{"kind":"loyalty","amount":-2}`.
    ///
    /// The one cost in the IR that can be *positive*: paying `+1` adds a loyalty
    /// counter rather than removing one, and CR 118 is comfortable with that because a
    /// loyalty symbol is a cost by definition, not by direction. A negative amount is
    /// payable only while the source has at least that many loyalty counters
    /// (CR 606.3), which is what makes `−7` unofferable on a planeswalker at 4.
    ///
    /// Its presence is what makes an ability a **loyalty ability**
    /// ([`is_loyalty_ability`]), and that carries two timing rules no other activated
    /// ability has: sorcery speed, and once per turn per permanent (CR 606.3). Both are
    /// enforced where every other activation gate is — the offer in
    /// [`valid_actions`](crate::valid_actions) and the independent re-check in
    /// [`apply_action`](crate::apply_action).
    Loyalty {
        /// The signed change to the source's loyalty counters. Positive adds, negative
        /// removes, zero is the `0:` ability that costs nothing and is still an
        /// activation for the once-per-turn rule.
        amount: i32,
    },
    /// **Sacrifice the source** (CR 701.17) — the `Sacrifice this creature:` of a
    /// permanent that spends itself, authored as `{"kind":"sacrifice_this"}`.
    ///
    /// The source is not a *target* and is not chosen: the cost names one permanent and
    /// the ability is only ever activated from it, so there is nothing to pick. That is
    /// why this is a distinct variant rather than a selector with a "this" arm — a cost
    /// that requires a choice needs the choice to ride on the action, and this one never
    /// does.
    ///
    /// Always payable: an ability is only offered from a permanent on the battlefield,
    /// and a permanent on the battlefield can always be sacrificed. It is applied
    /// **last** among the costs, whatever order they are written in, so a `{T}` beside
    /// it still taps a permanent that is still there (CR 601.2h — costs are paid
    /// simultaneously, and the engine has to pick some order to write them down in).
    ///
    /// Sacrificing is a real death (CR 701.17b): the permanent goes to its owner's
    /// graveyard through the one leaves-battlefield seam, so a dies trigger — including
    /// the source's own — sees it exactly as it sees a creature destroyed in combat. The
    /// ability itself is unaffected: it is on the stack independently of its source
    /// (CR 113.7a), so it still resolves.
    SacrificeThis,
    /// **Remove `count` counters of `counter` from the source** (CR 118.3) — the
    /// `Remove a wish counter from this creature:` of a permanent that enters with a
    /// charge and spends it down, authored as
    /// `{"kind":"remove_counters","counter":"charge","count":1}`.
    ///
    /// Payable only while the source actually has that many, which is the whole content
    /// of a charge-counter card: the ability is offered three times and then stops being
    /// offered. Like [`Self::Loyalty`] it is checked twice — once when the action is
    /// offered and again, independently, when it is applied — so a forged activation
    /// cannot spend counters that are not there.
    ///
    /// Deliberately **not** the same variant as [`Self::Loyalty`], despite both moving
    /// counters. A loyalty cost is signed, may *add*, and carries CR 606.3's two timing
    /// rules; this one only ever removes and carries none of them. Collapsing the two
    /// would make a charge counter a loyalty ability.
    RemoveCounters {
        /// The kind of counter removed. Named `counter` on the wire because the enum
        /// already reserves the `kind` tag for its own discriminant.
        counter: CounterKind,
        /// How many of them are removed. At least one on every printed card.
        count: u32,
    },
    /// **Sacrifice a permanent the activating player picks** (CR 601.2b / 701.17) — the
    /// `Sacrifice another creature:` of a creature that eats its friends and the
    /// `Sacrifice a Goblin:` of one that eats its kin, authored as
    /// `{"kind":"sacrifice","card_type":"creature","another":true}`.
    ///
    /// The sibling of [`Self::SacrificeThis`] and a **separate variant** for the one
    /// reason that matters: this one requires a *choice*, and a choice has to ride on the
    /// action. A cost is paid as the ability is activated (CR 602.2b), so there is no
    /// resolution to ask during and nothing to take back once the ability is on the
    /// stack — the chosen permanent arrives in
    /// [`Action::ActivateAbility::payment`](crate::Action::ActivateAbility) or the
    /// activation does not happen.
    ///
    /// **Whose permanent stays a rule rather than a field**: CR 701.17b lets a player
    /// sacrifice only what they control, so there is nothing to author and nothing to get
    /// wrong. How *many* is a field ([`count`](Self::Sacrifice::count)) because a printed
    /// card really does vary it — `Sacrifice two artifacts` is one cost taking a pair, not
    /// two costs taking one each, and writing it as two would let a player pay half of it.
    ///
    /// Payable only while enough matching permanents exist, so an ability with nothing to
    /// feed it is simply not offered (CR 602.2b) rather than offered and then found free.
    /// Paying it is a **real death** down the one leaves-battlefield seam, exactly as
    /// [`Self::SacrificeThis`] is.
    Sacrifice {
        /// The card type a matching permanent must have — the `creature` of `Sacrifice
        /// another creature`. Absent means any type, which is what `Sacrifice a Goblin`
        /// means: a Goblin is a Goblin whatever else it is.
        #[serde(default)]
        card_type: Option<CardType>,
        /// The printed subtype a matching permanent must have — the `Goblin` of
        /// `Sacrifice a Goblin`. Absent means any subtype.
        #[serde(default)]
        subtype: Option<String>,
        /// Whether the source is excluded — the **another** of `Sacrifice another
        /// creature`. `false` lets an ability eat its own source, which is what
        /// `Sacrifice a Goblin` on a Goblin means and is a legal (if final) activation.
        #[serde(default)]
        another: bool,
        /// How many permanents it takes. Defaults to exactly one, which is what every
        /// card written before the count existed says.
        #[serde(default)]
        count: SacrificeCount,
    },
    /// **Exile `count` cards the activating player picks from their own graveyard**
    /// (CR 601.2b / 701.19) — the `Exile a creature card from your graveyard:` of a
    /// permanent that eats the pile, authored as
    /// `{"kind":"exile_from_graveyard","class":"creature"}`.
    ///
    /// The third cost whose payment is a *choice*, and the first that spends an object
    /// which is not on the battlefield. It is not a [`Self::Sacrifice`] with a different
    /// destination: a card in a graveyard has no [`PermanentId`](crate::PermanentId), no
    /// controller, and no computed characteristics, so what may pay it is read off the
    /// **printed** face exactly as a graveyard target's is
    /// ([`GraveyardCardClass`]).
    ///
    /// **Whose graveyard stays a rule rather than a field**, as whose permanent does for a
    /// sacrifice: every printed cost of this shape says *your graveyard*, and a cost that
    /// reached across the table would be a different card.
    ///
    /// Payable only while enough matching cards are there, so the ability stops being
    /// offered when the pile runs out — which is the whole of what a card like this does.
    /// Paying it moves the card to its owner's exile zone; it is not a death and fires no
    /// dies trigger, because nothing died.
    ExileFromGraveyard {
        /// The class of card that may pay it — the `creature` of `Exile a creature card
        /// from your graveyard`. Defaults to any card.
        #[serde(default)]
        class: GraveyardCardClass,
        /// How many cards are exiled. At least one on every printed card.
        #[serde(default = "one")]
        count: u8,
        /// Whether the source itself is excluded — the **other** of `Exile seven other
        /// cards from your graveyard` (issue #723).
        ///
        /// It matters only where the source is *in* that graveyard, which is exactly the
        /// ability that prints it: a card paying to return itself must not be allowed to
        /// pay with itself, or it exiles the card it was about to bring back and the
        /// ability does nothing. The [`Cost::Sacrifice::another`] of the battlefield side,
        /// one zone over.
        #[serde(default)]
        another: bool,
    },
    /// **Discard `count` cards** from the activating player's hand (CR 601.2b / 701.8) —
    /// the `{T}, Discard a card:` of a rummaging creature, authored as
    /// `{"kind":"discard","count":1}`.
    ///
    /// The hand counterpart of [`Self::Sacrifice`], carrying its choice the same way and
    /// for the same reason. Payable only out of cards the activator actually holds, so an
    /// empty hand withholds the offer; unlike the cast-side cost there is no card to
    /// exclude, because the source of an activated ability is on the battlefield rather
    /// than on its way to the stack.
    Discard {
        /// How many cards are discarded. At least one on every printed card.
        count: u8,
    },
}

/// What accepting a `you may …` charges (CR 608.2) — the activation vocabulary minus
/// every cost whose payment is the **source itself**.
///
/// A separate enum from [`Cost`] rather than a reuse of it, and the line between them is
/// not the payment but the *moment*. An activation cost is paid by the player taking an
/// action, with the source in front of them: tapping it, moving its loyalty, sacrificing
/// it, taking counters off it are all things they can point at. An optional cost is paid
/// in the middle of somebody's resolution, from a question on a queue that carries no
/// source — the object that asked may already have left (CR 608.2), and a spell never had
/// a permanent to begin with. Those four costs are therefore not *rejected* here, they are
/// unwritable: `{"kind":"tap"}` under a `may` fails to parse, in `build.rs` and in the
/// loader, rather than authoring a card that looks fine and resolves into nothing.
///
/// What is left is exactly the three payments a player can make with no source to name,
/// deserialized with the same internal `kind` tag a [`Cost`] uses:
/// `{"kind":"mana","mana":"{1}"}`, `{"kind":"sacrifice","card_type":"creature",
/// "another":true}`, `{"kind":"discard","count":1}`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OptionalCost {
    /// Pay mana — the `you may pay {1}` of a card that offers a draw for a mana.
    ///
    /// Charged from the chooser's pool at the moment they accept, through the one
    /// [`ManaPool::pay`](crate::ManaPool::pay) seam a cast and an activation use. This is
    /// the only place mana moves outside the cast and activation paths, and the reason a
    /// player owed this question may still activate mana abilities (CR 605.3a).
    Mana {
        /// The mana cost in curly-brace notation, named `mana` for the reason
        /// [`Cost::Mana`]'s field is.
        mana: String,
    },
    /// **Sacrifice a permanent the chooser picks** (CR 701.17) — the `you may sacrifice
    /// another creature` of a creature that eats a friend for a pump.
    ///
    /// Unlike its activation-cost sibling the payment does *not* ride on the action:
    /// there is no announcement to carry it, because the question is asked while an
    /// object resolves. Accepting poses a second question — the ordinary
    /// [`ChoiceQuestion::Permanents`](crate::ChoiceQuestion) a mandatory sacrifice uses —
    /// and the wrapped effects wait behind it, so the payment happens before what it
    /// bought, down the same real-death seam (CR 701.17b).
    Sacrifice {
        /// The card type a matching permanent must have — the `creature` of `sacrifice
        /// another creature`. Absent means any type.
        #[serde(default)]
        card_type: Option<CardType>,
        /// The printed subtype a matching permanent must have — the `Goblin` of
        /// `sacrifice a Goblin`. Absent means any subtype.
        #[serde(default)]
        subtype: Option<String>,
        /// Whether the source is excluded — the **another** of `sacrifice another
        /// creature`. Answered against the permanent the asking ability came from,
        /// resolved when the question is posed.
        #[serde(default)]
        another: bool,
    },
    /// **Sacrifice the asking ability's own source** (CR 701.17) — the `you may sacrifice
    /// this enchantment` of a card that trades itself for what it makes.
    ///
    /// The one optional cost that asks **nothing further**: the permanent is named by the
    /// sentence rather than picked, so accepting pays it outright where its siblings pose
    /// a second question. It is the `sacrifice_this` activation cost's counterpart, and
    /// differs from it exactly as [`Self::Sacrifice`] differs from its own: a cost is paid
    /// as an ability is activated, and this happens while one resolves.
    ///
    /// A source that has already left the battlefield cannot pay it — which is a decline
    /// rather than a free effect, so a card whose enchantment was destroyed in response
    /// makes no token.
    SacrificeThis,
    /// **Discard `count` cards** from the chooser's hand (CR 701.8) — the hand
    /// counterpart of [`Self::Sacrifice`], paid the same way through a
    /// [`ChoiceQuestion::Cards`](crate::ChoiceQuestion) posed on acceptance.
    Discard {
        /// How many cards are discarded. At least one on every printed card.
        count: u8,
    },
}

impl OptionalCost {
    /// The activation-cost component this payment is.
    ///
    /// Exists for one reason: the words. "Sacrifice another creature" is one phrase
    /// whether a card prints it before a colon or after a `you may`, and the formatter
    /// that writes it (the server's `cost_symbol`) is already written once against
    /// [`Cost`]. Mapping onto that enum keeps the printed cost line, the offer's label,
    /// and the prompt the payment is answered on one string rather than three that have
    /// to be kept in step.
    #[must_use]
    pub fn as_activation_cost(&self) -> Cost {
        match self {
            OptionalCost::Mana { mana } => Cost::Mana { mana: mana.clone() },
            OptionalCost::Sacrifice {
                card_type,
                subtype,
                another,
            } => Cost::Sacrifice {
                card_type: *card_type,
                subtype: subtype.clone(),
                another: *another,
                // An optional cost takes exactly one permanent, which is every count a
                // cost may name: how many is not a question a payment asks.
                count: SacrificeCount::Exactly(1),
            },
            OptionalCost::Discard { count } => Cost::Discard { count: *count },
            OptionalCost::SacrificeThis => Cost::SacrificeThis,
        }
    }

    /// The mana this cost charges, or `None` for one paid with something else.
    ///
    /// The one question two different callers ask — whether the offer is judged against a
    /// mana pool, and whether accepting charges one — so it is answered here rather than
    /// matched for at each of them.
    #[must_use]
    pub fn mana(&self) -> Option<&str> {
        match self {
            OptionalCost::Mana { mana } => Some(mana),
            OptionalCost::Sacrifice { .. }
            | OptionalCost::SacrificeThis
            | OptionalCost::Discard { .. } => None,
        }
    }
}

impl Cost {
    /// How many cards this component discards, or `0` for a component that discards none.
    ///
    /// A named accessor for the reason the cast side's is: the offer gate, the candidate
    /// list, and the payment check each ask one question in one place rather than matching
    /// the enum for themselves.
    #[must_use]
    pub fn discard_count(&self) -> u8 {
        match self {
            Cost::Discard { count } => *count,
            Cost::Tap
            | Cost::Mana { .. }
            | Cost::Loyalty { .. }
            | Cost::SacrificeThis
            | Cost::RemoveCounters { .. }
            | Cost::ExileFromGraveyard { .. }
            | Cost::Sacrifice { .. } => 0,
        }
    }

    /// How many cards this component exiles from its payer's graveyard, or `0` for a
    /// component that exiles none.
    ///
    /// The graveyard counterpart of [`Self::discard_count`], and a named accessor for the
    /// same reason: the offer gate, the candidate list, and the payment check each ask one
    /// question in one place rather than matching the enum for themselves.
    #[must_use]
    pub fn exile_count(&self) -> u8 {
        match self {
            Cost::ExileFromGraveyard { count, .. } => *count,
            Cost::Tap
            | Cost::Mana { .. }
            | Cost::Loyalty { .. }
            | Cost::SacrificeThis
            | Cost::RemoveCounters { .. }
            | Cost::Discard { .. }
            | Cost::Sacrifice { .. } => 0,
        }
    }

    /// How many permanents this component sacrifices, or `None` for a component that
    /// sacrifices none the player picks.
    ///
    /// `None` for [`Cost::SacrificeThis`] too, and deliberately: that one names its own
    /// source and carries no choice, so it is answered where the source is rather than out
    /// of a payment.
    #[must_use]
    pub fn sacrifice_count(&self) -> Option<SacrificeCount> {
        match self {
            Cost::Sacrifice { count, .. } => Some(*count),
            Cost::Tap
            | Cost::Mana { .. }
            | Cost::Loyalty { .. }
            | Cost::SacrificeThis
            | Cost::RemoveCounters { .. }
            | Cost::ExileFromGraveyard { .. }
            | Cost::Discard { .. } => None,
        }
    }

    /// Whether `data` is a card this component would accept as its exile payment —
    /// everything about a candidate that is a question about the *card*, with whose
    /// graveyard it is in left to the caller that knows the zones.
    ///
    /// `false` for every component that exiles nothing, so a caller may ask it of a whole
    /// cost list without first sorting out which entry is which.
    #[must_use]
    pub fn accepts_exile(&self, data: &crate::CardData) -> bool {
        match self {
            Cost::ExileFromGraveyard { class, .. } => class.matches(data),
            _ => false,
        }
    }

    /// Whether `face` is a permanent this component would accept as its sacrifice —
    /// everything about a candidate that is a question about the *card*, with whose it is
    /// and whether it is the source left to the caller that knows the board.
    ///
    /// `false` for every component that sacrifices nothing, so a caller may ask it of a
    /// whole cost list without first sorting out which entry is which.
    #[must_use]
    pub fn accepts_sacrifice(&self, face: &crate::token::PrintedFace<'_>) -> bool {
        let Cost::Sacrifice {
            card_type, subtype, ..
        } = self
        else {
            return false;
        };
        card_type.is_none_or(|wanted| face.has_type(wanted))
            && subtype
                .as_deref()
                .is_none_or(|wanted| face.has_subtype(wanted))
    }

    /// Whether this component excludes the ability's own source — the `another` of
    /// `Sacrifice another creature`. `false` for every component that sacrifices nothing.
    #[must_use]
    pub fn excludes_source(&self) -> bool {
        matches!(self, Cost::Sacrifice { another: true, .. })
    }
}
