//! What an activated ability charges to activate (CR 118): tapping, mana, and loyalty.

use super::*;

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
}
