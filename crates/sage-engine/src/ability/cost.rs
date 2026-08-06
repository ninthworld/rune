//! What an activated ability charges to activate (CR 118): tapping, mana, loyalty, and
//! the three costs whose payment the activating player **chooses**.

use super::*;

/// How many permanents a sacrifice cost takes (CR 601.2b).
///
/// One vocabulary for both places a cost sacrifices something the player picks — an
/// activation's [`Cost::Sacrifice`] and a cast's
/// [`AdditionalCost::Sacrifice`](crate::AdditionalCost) — because "how many" is the same
/// question of both and a second spelling would be a second thing to get wrong.
///
/// Authored the way [`ObservedSpell`] is, externally tagged: `{"exactly": 2}` for a fixed
/// number and `"any"` for the open one. Absent defaults to exactly one, which is what
/// every card written before this existed says.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SacrificeCount {
    /// Exactly this many, never fewer and never more — the `two artifacts` of an
    /// activation that eats a pair. Over-paying a cost is not something a player may
    /// choose to do, so this is exact in both directions.
    Exactly(u8),
    /// **Any number the payer chooses, including none** — the `Sacrifice any number of
    /// lands` of a spell that trades a board for a search.
    ///
    /// Zero is a legal payment, which is what makes such a cost never a reason to
    /// withhold the offer: there is always something to pay it with, even on an empty
    /// board. It is also the only cost in the IR whose *size* is a decision, which is why
    /// the number it settled on has to be recorded as it is paid
    /// ([`PaidCost`](crate::PaidCost)) rather than counted again later.
    Any,
}

impl Default for SacrificeCount {
    fn default() -> Self {
        Self::Exactly(1)
    }
}

impl SacrificeCount {
    /// The fewest permanents this count accepts.
    #[must_use]
    pub fn min(self) -> u8 {
        match self {
            Self::Exactly(count) => count,
            Self::Any => 0,
        }
    }

    /// Whether `paid` permanents is exactly what this count asks for, given that
    /// `available` could have been sacrificed.
    ///
    /// The one place the two shapes differ, so no caller has to match the enum itself: a
    /// fixed count is met by that many, and an open one by anything up to what the board
    /// held. An open count can never be over-paid either — there is nothing left to name
    /// once every candidate has been named.
    #[must_use]
    pub fn is_paid_by(self, paid: usize, available: usize) -> bool {
        match self {
            Self::Exactly(count) => paid == usize::from(count),
            Self::Any => paid <= available,
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
