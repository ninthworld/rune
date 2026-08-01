//! Additional costs to cast a card (CR 601.2b).

use serde::Deserialize;

use crate::id::PlayerId;
use crate::state::GameState;

/// An **additional cost** a card's own text imposes on casting it (CR 601.2b): the
/// `As an additional cost to cast this spell, discard a card.` of a rummaging draw
/// spell.
///
/// A cost, not an effect — and the difference is the whole reason this type exists.
/// Written as an effect the discard would happen on **resolution**, which makes the
/// spell castable with an empty hand, castable while the discard is countered away,
/// and free to be responded to before the card is gone. As a cost it is paid while the
/// spell is being cast (CR 601.2h): the card cannot even be *offered* unless the cost
/// can be paid ([`GameState::additional_cost_is_payable`]), and paying it is part of
/// the cast rather than something the stack could interrupt.
///
/// Deliberately small and closed, deserialized with an internal `kind` tag:
/// `{"kind": "discard", "count": 1}`. It grows by adding variants as cards need them;
/// sacrificing and exiling as a cost are not modeled (`data/exclusions.json`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AdditionalCost {
    /// Discard `count` cards from the caster's hand as the spell is cast.
    ///
    /// The cards are chosen by the caster through the ordinary mid-resolution choice
    /// mechanism ([`crate::PendingChoice`]) posed at the moment of casting, so the
    /// question, the candidate set, and the hidden-zone discipline are the ones every
    /// other discard already uses. The spell itself is already on the stack when the
    /// question is asked, so it can never be discarded to its own cost.
    Discard {
        /// How many cards must be discarded. Always at least one on a real card; a
        /// zero-count cost is no cost and the catalog validator rejects it.
        count: u8,
    },
}

impl AdditionalCost {
    /// How many cards this cost discards, or `0` for a cost that discards nothing.
    #[must_use]
    pub fn discard_count(self) -> u8 {
        match self {
            AdditionalCost::Discard { count } => count,
        }
    }
}

impl GameState {
    /// Whether `player` could pay `cost` right now, casting `casting` — the predicate
    /// the cast offer is gated on (CR 601.2b: a cost that cannot be paid makes the
    /// spell uncastable, not castable-and-then-skipped).
    ///
    /// The card being cast is **excluded** from its own payment: it is on its way to
    /// the stack, so a hand of exactly this one card cannot discard to cast it.
    #[must_use]
    pub fn additional_cost_is_payable(
        &self,
        player: PlayerId,
        cost: AdditionalCost,
        casting: crate::id::CardInstanceId,
    ) -> bool {
        match cost {
            AdditionalCost::Discard { count } => {
                let available = self
                    .players
                    .get(player.0)
                    .map_or(0, |p| p.hand.iter().filter(|c| c.id != casting).count());
                available >= usize::from(count)
            }
        }
    }
}
