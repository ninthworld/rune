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
}
