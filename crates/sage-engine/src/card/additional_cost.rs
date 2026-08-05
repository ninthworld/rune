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
/// `{"kind": "discard", "count": 1}`, `{"kind": "sacrifice", "card_type": "creature"}`.
/// It grows by adding variants as cards need them; exiling as a cost is not modeled
/// (`data/exclusions.json`).
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
    /// Sacrifice one permanent of `card_type` as the spell is cast (CR 701.17).
    ///
    /// The permanent is chosen by the caster and named in the action's payment, exactly
    /// as a discarded card is: a cost paid at announcement has to carry its choice on
    /// the action, because there is no resolution yet to ask during and nothing to take
    /// back once the spell is on the stack.
    ///
    /// **You may only sacrifice a permanent you control** (CR 701.17b), so whose
    /// permanent it is stays a rule rather than a field — there is no authoring of it to
    /// get wrong. Which *one* is always exactly one: no card in this set sacrifices two,
    /// and a count would be a number every reader had to carry for no card.
    Sacrifice {
        /// The card type the sacrificed permanent must have — the "creature" of
        /// `As an additional cost to cast this spell, sacrifice a creature.`
        card_type: crate::card_type::CardType,
    },
}

impl AdditionalCost {
    /// How many cards this cost discards, or `0` for a cost that discards nothing.
    #[must_use]
    pub fn discard_count(self) -> u8 {
        match self {
            AdditionalCost::Discard { count } => count,
            AdditionalCost::Sacrifice { .. } => 0,
        }
    }

    /// The card type this cost sacrifices a permanent of, or `None` for a cost that
    /// sacrifices nothing.
    ///
    /// The sibling of [`Self::discard_count`], and a named accessor for the same reason:
    /// the legality check, the candidate list, and the payment all ask one question in
    /// one place rather than each matching the enum for themselves.
    #[must_use]
    pub fn sacrifice_type(self) -> Option<crate::card_type::CardType> {
        match self {
            AdditionalCost::Sacrifice { card_type } => Some(card_type),
            AdditionalCost::Discard { .. } => None,
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
        db: &crate::card::CardDatabase,
    ) -> bool {
        match cost {
            AdditionalCost::Discard { count } => {
                let available = self
                    .players
                    .get(player.0)
                    .map_or(0, |p| p.hand.iter().filter(|c| c.id != casting).count());
                available >= usize::from(count)
            }
            // A permanent on the battlefield is never the card being cast — that one is
            // in hand, on its way to the stack — so unlike the discard above there is
            // nothing to exclude here.
            AdditionalCost::Sacrifice { card_type } => self.battlefield.iter().any(|perm| {
                perm.controller == player
                    && perm
                        .printed
                        .face(db)
                        .is_some_and(|face| face.has_type(card_type))
            }),
        }
    }
}
